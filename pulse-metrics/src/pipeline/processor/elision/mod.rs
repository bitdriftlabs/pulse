// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./mod_test.rs"]
mod mod_test;

use self::get_last_elided::GetLastElided;
use self::state::{ElisionState, SkipDecider, SkipDecision};
use super::PipelineProcessor;
use crate::admin::server::Admin;
use crate::filters::filter::{DynamicMetricFilter, MetricFilterDecision};
use crate::filters::fst::FstFilterProvider;
use crate::filters::poll_filter::PollFilter;
use crate::filters::regex::RegexFilterProvider;
use crate::filters::zero::ZeroFilter;
use crate::pipeline::PipelineDispatch;
use crate::pipeline::processor::ProcessorFactoryContext;
use crate::protos::metric::ParsedMetric;
use anyhow::bail;
use async_trait::async_trait;
use bd_server_stats::stats::Scope;
use bd_shutdown::ComponentShutdownTriggerHandle;
use elision::ElisionConfig;
use elision::elision_config::emit_config::Emit_type;
use elision::elision_config::{
  BlocklistConfig,
  EmitConfig,
  RegexOverrideConfig,
  ZeroElisionConfig,
};
use log::{error, trace};
use parking_lot::Mutex;
use prometheus::IntCounter;
use pulse_common::singleton::SingletonManager;
use pulse_protobuf::protos::pulse::config::processor::v1::elision;
use regex::Regex;
use regex::bytes::RegexSet;
use std::collections::HashMap;
use std::sync::Arc;

pub mod get_last_elided;
pub mod state;

#[derive(Clone)]
pub struct OverrideConfig {
  override_keys: RegexSet,
  override_values: Vec<EmitConfig>,
}

impl OverrideConfig {
  pub fn new(regex_overrides: &[RegexOverrideConfig]) -> anyhow::Result<Self> {
    let mut override_keys: Vec<_> = Vec::new();
    let mut override_values: Vec<EmitConfig> = Vec::new();
    for o in regex_overrides {
      override_keys.push(o.regex.clone());
      override_values.push(o.emit.as_ref().unwrap().clone());
    }

    let regex_set = RegexSet::new(override_keys)?;

    Ok(Self {
      override_keys: regex_set,
      override_values,
    })
  }

  #[must_use]
  pub fn get_override(&self, name: &[u8]) -> Option<&EmitConfig> {
    let matches = self.override_keys.matches_at(name, 0);

    if !matches.matched_any() {
      return None;
    }

    Some(&self.override_values[matches.iter().next().unwrap()])
  }
}

fn build_filter_scope(name: &str, scope: &Scope) -> Scope {
  let mut labels = HashMap::new();
  labels.insert("filter_name".to_string(), name.to_string());
  scope.scope_with_labels("", labels)
}

fn build_filter_counter(f: &DynamicMetricFilter, scope: &Scope) -> IntCounter {
  let mut labels = HashMap::new();
  labels.insert(
    "match_decision".to_string(),
    match f.match_decision() {
      MetricFilterDecision::Pass => "pass",
      MetricFilterDecision::Fail => "fail",
      _ => unreachable!(),
    }
    .to_string(),
  );
  scope.counter_with_labels("considered", labels)
}

async fn build_metric_filters(
  config: ElisionProcessorConfig,
  s: Scope,
  shutdown_trigger_handle: ComponentShutdownTriggerHandle,
  singleton_manager: &SingletonManager,
  admin: Arc<dyn Admin>,
) -> anyhow::Result<(Vec<DynamicMetricFilter>, Vec<IntCounter>)> {
  let scope = s.scope("filter");
  let mut metric_filters: Vec<DynamicMetricFilter> = vec![];
  let mut metric_filter_stats: Vec<IntCounter> = vec![];

  let elide_zero = config.zero.enabled.unwrap_or(true);
  if elide_zero {
    let f: DynamicMetricFilter = Arc::new(ZeroFilter::new("ZeroFilter", &config.zero));
    let filter_scope = build_filter_scope(f.name(), &scope);
    metric_filter_stats.push(build_filter_counter(&f, &filter_scope));
    metric_filters.push(f);
  }

  if let Some(blocklist_config) = config.blocklist {
    if let Some(source) = blocklist_config.allowed_metrics.as_ref() {
      let filter_name = "exact_allowlist";
      let filter_scope = build_filter_scope(filter_name, &scope);
      match PollFilter::try_from_config(
        |file| async { FstFilterProvider::new(file).await },
        &filter_scope,
        source.clone(),
        blocklist_config.prom,
        filter_name,
        MetricFilterDecision::Pass,
        shutdown_trigger_handle.make_shutdown(),
        singleton_manager,
        admin.clone(),
      )
      .await
      {
        Ok(metric_filter) => {
          let f: DynamicMetricFilter = metric_filter;
          metric_filter_stats.push(build_filter_counter(&f, &filter_scope));
          metric_filters.push(f);
        },
        Err(e) => {
          error!("failed to load metric allowlist fst filter: {}", e);
          return Err(e);
        },
      }
    }

    if let Some(source) = blocklist_config.allowed_metric_patterns.as_ref() {
      let filter_name = "regex_allowlist";
      let filter_scope = build_filter_scope(filter_name, &scope);
      match PollFilter::try_from_config(
        |file| async { RegexFilterProvider::new(file).await },
        &filter_scope,
        source.clone(),
        blocklist_config.prom,
        filter_name,
        MetricFilterDecision::Pass,
        shutdown_trigger_handle.make_shutdown(),
        singleton_manager,
        admin.clone(),
      )
      .await
      {
        Ok(f) => {
          let f: DynamicMetricFilter = f;
          metric_filter_stats.push(build_filter_counter(&f, &filter_scope));
          metric_filters.push(f);
        },
        Err(e) => {
          error!("failed to load metric allowlist regex filter: {}", e);
          return Err(e);
        },
      }
    }

    let filter_name = "blocklist";
    let filter_scope = build_filter_scope(filter_name, &scope);
    match PollFilter::try_from_config(
      |file| async { FstFilterProvider::new(file).await },
      &filter_scope,
      blocklist_config.blocked_metrics.unwrap(),
      blocklist_config.prom,
      filter_name,
      MetricFilterDecision::Fail,
      shutdown_trigger_handle.make_shutdown(),
      singleton_manager,
      admin,
    )
    .await
    {
      Ok(metric_filter) => {
        let f: DynamicMetricFilter = metric_filter;
        metric_filter_stats.push(build_filter_counter(&f, &filter_scope));
        metric_filters.push(f);
      },
      Err(e) => {
        error!("failed to load metric blocklist fst filter: {}", e);
        return Err(e);
      },
    }
  }

  Ok((metric_filters, metric_filter_stats))
}

#[derive(Clone)]
pub struct ElisionProcessorConfig {
  pub override_config: OverrideConfig,
  pub analyze_mode: bool,
  pub zero: ZeroElisionConfig,
  pub blocklist: Option<BlocklistConfig>,
}

pub struct ElisionProcessor {
  config: ElisionProcessorConfig,
  state: ElisionState,
  metric_filters: Vec<DynamicMetricFilter>,
  metric_filter_stats: Vec<IntCounter>,
  skip_decider: SkipDecider,
  dispatcher: Arc<dyn PipelineDispatch>,
  get_last_elided: Mutex<Option<Arc<GetLastElided>>>,
}

impl ElisionProcessor {
  fn validate_emit(emit: &EmitConfig) -> anyhow::Result<()> {
    if let Emit_type::Ratio(ratio) = emit.emit_type.as_ref().unwrap() {
      if *ratio <= 0.0 || *ratio > 1.0 {
        bail!("elision ratio must be > 0.0 and <= 1.0");
      }
    }
    Ok(())
  }

  fn validate(config: &ElisionConfig) -> anyhow::Result<()> {
    Self::validate_emit(config.emit.as_ref().unwrap())?;
    for regex_override in &config.regex_overrides {
      Self::validate_emit(regex_override.emit.as_ref().unwrap())?;
      Regex::new(regex_override.regex.as_str())?;
    }

    Ok(())
  }

  #[allow(clippy::too_many_arguments)]
  pub async fn new(
    config: ElisionConfig,
    context: ProcessorFactoryContext,
  ) -> anyhow::Result<Arc<Self>> {
    Self::validate(&config)?;

    let state = ElisionState::new(context.metric_cache, &context.scope);
    let skip_decider = SkipDecider::new(config.emit.unwrap());
    let override_config = OverrideConfig::new(&config.regex_overrides)?;
    let config = ElisionProcessorConfig {
      override_config,
      analyze_mode: config.analyze_mode,
      zero: config.zero.unwrap_or_default(),
      blocklist: config.blocklist.into_option(),
    };
    let (metric_filters, metric_filter_stats) = build_metric_filters(
      config.clone(),
      context.scope,
      context.shutdown_trigger_handle,
      &context.singleton_manager,
      context.admin.clone(),
    )
    .await?;

    let processor = Arc::new(Self {
      config,
      state,
      metric_filters,
      metric_filter_stats,
      skip_decider,
      dispatcher: context.dispatcher,
      get_last_elided: Mutex::default(),
    });

    let get_last_elided = GetLastElided::register_elision(
      processor.clone(),
      &context.singleton_manager,
      context.admin.as_ref(),
    )
    .await;
    *processor.get_last_elided.lock() = Some(get_last_elided);

    Ok(processor)
  }

  fn decide(&self, sample: &ParsedMetric) -> SkipDecision {
    let (decision, filter_index) = self.state.update_and_decide(
      sample,
      &self.metric_filters,
      &self.skip_decider,
      self
        .config
        .override_config
        .get_override(sample.metric().get_id().name().as_ref()),
    );

    if let Some(c) = filter_index.and_then(|i| self.metric_filter_stats.get(i)) {
      c.inc();
    }

    decision
  }

  fn filter(&self, samples: Vec<ParsedMetric>) -> Vec<ParsedMetric> {
    let out: Vec<_> = samples
      .into_iter()
      .filter(|sample| {
        let decision = self.decide(sample);
        trace!(
          "sample={} ts={} decision={:?}",
          sample.metric().get_id(),
          sample.metric().timestamp,
          decision
        );
        match (decision, self.config.analyze_mode) {
          (SkipDecision::DoNotOutput, true) | (SkipDecision::Output, _) => true,
          (SkipDecision::DoNotOutput, false) => false,
        }
      })
      .collect();
    out
  }

  pub fn get_last_elided(&self, name: &str) -> anyhow::Result<Option<u64>> {
    Ok(Some(self.state.get_last_elided(name)))
  }
}

#[async_trait]
impl PipelineProcessor for ElisionProcessor {
  async fn recv_samples(self: Arc<Self>, samples: Vec<ParsedMetric>) {
    let filtered_samples = self.filter(samples);
    self.dispatcher.send(filtered_samples).await;
  }

  async fn start(self: Arc<Self>) {}
}
