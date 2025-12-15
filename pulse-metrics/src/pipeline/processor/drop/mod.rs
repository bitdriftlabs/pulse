// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./mod_test.rs"]
mod mod_test;

use super::{PipelineProcessor, ProcessorFactoryContext};
use crate::file_watcher::{FileWatcher, get_file_watcher};
use crate::pipeline::PipelineDispatch;
use crate::protos::metric::ParsedMetric;
use async_trait::async_trait;
use bd_server_stats::stats::Scope;
use drop::drop_processor_config::Config_source;
use drop::drop_rule::drop_condition::Condition_type;
use drop::drop_rule::string_match::String_match_type;
use drop::drop_rule::value_match::Value_match_type;
use drop::drop_rule::{DropCondition, DropMode, StringMatch, ValueMatch, ValueMatchOperator};
use drop::{DropConfig, DropProcessorConfig, DropRule};
use itertools::Itertools;
use parking_lot::RwLock;
use prometheus::{IntCounter, labels};
use pulse_common::proto::yaml_to_proto;
use pulse_protobuf::protos::pulse::config::processor::v1::drop;
use regex::bytes::Regex;
use std::sync::Arc;
use std::time::{Duration, Instant};

// TODO(mattklein123): There are many performance optimizations that we can do as follow ups,
// mainly around drop rules that only use name matches.
// 1) If all name matches are exact, we can use an FST.
// 2) Otherwise we can use a RegexSet and perform a single match across all names.

//
// TranslatedStringMatchCondition
//

enum TranslatedStringMatchCondition {
  Exact(String),
  Regex(Regex),
}

impl TranslatedStringMatchCondition {
  fn is_match(&self, value: &[u8]) -> bool {
    match self {
      Self::Exact(exact) => exact.as_bytes() == value,
      Self::Regex(regex) => regex.is_match(value),
    }
  }
}

//
// TranslatedDropCondition
//

enum TranslatedDropCondition {
  MetricName(TranslatedStringMatchCondition),
  TagMatch {
    name: String,
    value_match: Option<TranslatedStringMatchCondition>,
  },
  ValueMatch(ValueMatch),
  AndMatch(Vec<Self>),
  NotMatch(Box<Self>),
  TimestampAgeMatch(Duration),
}

impl TranslatedDropCondition {
  fn string_match_to_regex(
    string_match: &StringMatch,
  ) -> anyhow::Result<TranslatedStringMatchCondition> {
    Ok(
      match string_match.string_match_type.as_ref().expect("pgv") {
        String_match_type::Exact(exact) => TranslatedStringMatchCondition::Exact(exact.to_string()),
        String_match_type::Regex(regex) => {
          TranslatedStringMatchCondition::Regex(Regex::new(regex)?)
        },
      },
    )
  }

  fn new(condition: &DropCondition) -> anyhow::Result<Self> {
    match condition.condition_type.as_ref().expect("pgv") {
      Condition_type::MetricName(metric_name) => {
        Ok(Self::MetricName(Self::string_match_to_regex(metric_name)?))
      },
      Condition_type::TagMatch(tag_match) => Ok(Self::TagMatch {
        name: tag_match.tag_name.to_string(),
        value_match: tag_match
          .tag_value
          .as_ref()
          .map(Self::string_match_to_regex)
          .transpose()?,
      }),
      Condition_type::ValueMatch(value_match) => Ok(Self::ValueMatch(value_match.clone())),
      Condition_type::AndMatch(and_match) => {
        let translated_conditions = and_match.conditions.iter().map(Self::new).try_collect()?;
        Ok(Self::AndMatch(translated_conditions))
      },
      Condition_type::NotMatch(not_match) => {
        let translated_condition = Self::new(not_match.as_ref())?;
        Ok(Self::NotMatch(Box::new(translated_condition)))
      },
      Condition_type::TimestampAgeMatch(timestamp_age_match) => Ok(Self::TimestampAgeMatch(
        Duration::from_secs(timestamp_age_match.max_age_seconds),
      )),
    }
  }

  fn is_value_match(sample: &ParsedMetric, value_match: &ValueMatch) -> bool {
    // We can only do value matches for simple metrics.
    let Some(value) = sample.metric().value.maybe_simple() else {
      return false;
    };

    let Some(Value_match_type::SimpleValue(simple_value_match)) = &value_match.value_match_type
    else {
      return false;
    };

    match simple_value_match.operator.enum_value_or_default() {
      ValueMatchOperator::EQUAL => (value - simple_value_match.target).abs() < f64::EPSILON,
      ValueMatchOperator::NOT_EQUAL => (value - simple_value_match.target).abs() >= f64::EPSILON,
      ValueMatchOperator::GREATER => value > simple_value_match.target,
      ValueMatchOperator::GREATER_OR_EQUAL => value >= simple_value_match.target,
      ValueMatchOperator::LESS => value < simple_value_match.target,
      ValueMatchOperator::LESS_OR_EQUAL => value <= simple_value_match.target,
    }
  }

  fn is_timestamp_age_match(sample: &ParsedMetric, max_age: Duration) -> bool {
    let current_timestamp = crate::protos::metric::default_timestamp();
    let metric_timestamp = sample.metric().timestamp;

    // If the metric timestamp is in the future, don't drop it.
    if metric_timestamp > current_timestamp {
      return false;
    }

    let age_seconds = current_timestamp - metric_timestamp;
    age_seconds > max_age.as_secs()
  }

  fn drop_sample(&self, sample: &ParsedMetric) -> bool {
    match self {
      Self::MetricName(value_match) => value_match.is_match(sample.metric().get_id().name()),
      Self::TagMatch { name, value_match } => sample
        .metric()
        .get_id()
        .tag(name.as_str())
        .is_some_and(|tag_value| {
          value_match
            .as_ref()
            .is_none_or(|value_match| value_match.is_match(&tag_value.value))
        }),
      Self::ValueMatch(value_match) => Self::is_value_match(sample, value_match),
      Self::AndMatch(conditions) => conditions
        .iter()
        .all(|condition| condition.drop_sample(sample)),
      Self::NotMatch(condition) => !condition.drop_sample(sample),
      Self::TimestampAgeMatch(max_age) => Self::is_timestamp_age_match(sample, *max_age),
    }
  }
}

//
// TranslatedDropRule
//

struct TranslatedDropRule {
  name: String,
  mode: DropMode,
  conditions: Vec<TranslatedDropCondition>,
  drop_counter: IntCounter,
  warn_interval: Option<Duration>,
  last_warning_time: parking_lot::Mutex<Option<Instant>>,
}

impl TranslatedDropRule {
  fn new(rule: &DropRule, scope: &Scope) -> anyhow::Result<Self> {
    let warn_interval = if rule.warn_interval_seconds > 0 {
      Some(Duration::from_secs(rule.warn_interval_seconds))
    } else {
      None
    };

    Ok(Self {
      name: rule.name.to_string(),
      mode: rule.mode.enum_value_or_default(),
      conditions: rule
        .conditions
        .iter()
        .map(TranslatedDropCondition::new)
        .try_collect()?,
      drop_counter: scope.counter_with_labels(
        "dropped",
        labels! {
          "rule_name".to_string() => rule.name.to_string(),
          "mode".to_string() => match rule.mode.enum_value_or_default() {
            DropMode::ENABLED => "enabled".to_string(),
            DropMode::TESTING => "testing".to_string(),
          }
        },
      ),
      warn_interval,
      last_warning_time: parking_lot::Mutex::new(None),
    })
  }

  fn drop_sample(&self, sample: &ParsedMetric) -> bool {
    log::trace!(
      "checking drop rule {} mode {:?} for sample {}",
      self.name,
      self.mode,
      sample.metric()
    );
    let drop = self
      .conditions
      .iter()
      .any(|condition| condition.drop_sample(sample));
    if drop {
      // Determine the log level based on warning interval configuration
      let log_level = self
        .warn_interval
        .map_or(log::Level::Debug, |warn_interval| {
          let mut last_warning = self.last_warning_time.lock();
          let now = Instant::now();
          let should_warn =
            last_warning.is_none_or(|last| now.duration_since(last) >= warn_interval);

          if should_warn {
            *last_warning = Some(now);
            log::Level::Warn
          } else {
            log::Level::Debug
          }
        });

      log::log!(
        log_level,
        "dropping sample {} for rule {} mode {:?}",
        sample.metric(),
        self.name,
        self.mode
      );
      self.drop_counter.inc();
    }
    match self.mode {
      DropMode::ENABLED => drop,
      DropMode::TESTING => false,
    }
  }
}

//
// TranslatedDropConfig
//

pub struct TranslatedDropConfig {
  rules: Vec<TranslatedDropRule>,
}

impl TranslatedDropConfig {
  pub fn new(config: &DropConfig, scope: &Scope) -> anyhow::Result<Self> {
    let rules = config
      .rules
      .iter()
      .map(|rule| TranslatedDropRule::new(rule, scope))
      .try_collect()?;

    Ok(Self { rules })
  }

  pub fn drop_sample(&self, sample: &ParsedMetric) -> Option<&str> {
    self.rules.iter().find_map(|rule| {
      if rule.drop_sample(sample) {
        Some(rule.name.as_str())
      } else {
        None
      }
    })
  }
}

//
// DropProcessor
//

pub struct DropProcessor {
  dispatcher: Arc<dyn PipelineDispatch>,
  current_config: Arc<RwLock<TranslatedDropConfig>>,
}

impl DropProcessor {
  pub async fn new(
    config: DropProcessorConfig,
    context: ProcessorFactoryContext,
  ) -> anyhow::Result<Self> {
    let (translated_config, watcher) = match config.config_source.expect("pgv") {
      Config_source::Inline(drop_config) => (
        TranslatedDropConfig::new(&drop_config, &context.scope)?,
        None,
      ),
      Config_source::FileSource(file_source) => {
        log::info!("starting file watcher for drop config");
        let (watcher, initial) =
          get_file_watcher(file_source, context.shutdown_trigger_handle.make_shutdown()).await?;
        let drop_config: DropConfig = yaml_to_proto(str::from_utf8(&initial)?)?;
        (
          TranslatedDropConfig::new(&drop_config, &context.scope)?,
          Some(watcher),
        )
      },
    };

    let current_config = Arc::new(RwLock::new(translated_config));
    if let Some(mut watcher) = watcher {
      let cloned_current_config = current_config.clone();
      tokio::spawn(async move {
        loop {
          match Self::process_remote_file(&mut *watcher, &context.scope).await {
            Ok(config) => {
              log::info!("received new drop config from remote, updating");
              *cloned_current_config.write() = config;
            },
            Err(e) => {
              log::error!("Failed to process remote drop config: {e}");
            },
          }
        }
      });
    }

    Ok(Self {
      dispatcher: context.dispatcher,
      current_config,
    })
  }

  async fn process_remote_file(
    watcher: &mut dyn FileWatcher,
    scope: &Scope,
  ) -> anyhow::Result<TranslatedDropConfig> {
    let file = watcher.wait_until_modified().await?;
    let drop_config = yaml_to_proto(str::from_utf8(&file)?)?;
    TranslatedDropConfig::new(&drop_config, scope)
  }
}

#[async_trait]
impl PipelineProcessor for DropProcessor {
  async fn recv_samples(self: Arc<Self>, samples: Vec<ParsedMetric>) {
    log::debug!("received {} sample(s)", samples.len());
    let samples: Vec<_> = {
      let config = self.current_config.read();
      samples
        .into_iter()
        .filter(|sample| config.drop_sample(sample).is_none())
        .collect()
    };
    log::debug!("forwarding {} sample(s)", samples.len());
    self.dispatcher.send(samples).await;
  }

  async fn start(self: Arc<Self>) {}
}
