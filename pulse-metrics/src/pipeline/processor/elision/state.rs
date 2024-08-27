// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./state_test.rs"]
mod state_test;

use crate::filters::filter::{DynamicMetricFilter, MetricFilterDecision};
use crate::pipeline::metric_cache::{CachedMetric, GetOrInitResult, MetricCache, StateSlotHandle};
use crate::protos::metric::{Metric, MetricType, MetricValue, ParsedMetric};
use bd_server_stats::stats::Scope;
use elision::elision_config::emit_config::Emit_type;
use elision::elision_config::EmitConfig;
use parking_lot::Mutex;
use prometheus::{Histogram, IntCounter};
use pulse_protobuf::protos::pulse::config::processor::v1::elision;
use std::cmp::max;
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// State associated with [FilterDecision::PeriodicOutput] that is used to determine the
/// corresponding [SkipDecision].
#[derive(PartialEq, Hash, Eq, Debug)]
pub struct PeriodicOutputState {
  /// The number of times this metric has failed filter checks in succession.
  pub successive_fails: u64,
  /// The number of seconds since the metric was last emitted.
  pub seconds_since_last_emitted: u64,
}

impl PeriodicOutputState {
  const fn new(successive_fails: u64, seconds_since_last_emitted: u64) -> Self {
    Self {
      successive_fails,
      seconds_since_last_emitted,
    }
  }
}

#[derive(PartialEq, Hash, Eq, Debug)]
pub enum FilterDecision {
  Pass,
  PeriodicOutput(PeriodicOutputState),
  NotCovered,
  Overflow,
}

#[derive(PartialEq, Hash, Eq, Debug)]
pub enum SkipDecision {
  Output,
  DoNotOutput,
}

/// Internal elision state machine keeper for a single key
#[derive(Debug)]
struct State {
  /// The last reported value of this emitter, will always be set.
  last_value: Option<MetricValue>,
  /// The last reported type of this emitter.
  last_type: Option<MetricType>,
  /// The last reported timestamp, max(incoming_timestamp, last_timestamp).
  last_timestamp: u64,
  /// The number of times this metric has failed filter checks in succession.
  successive_fails: u64,
  /// The timestamp of the last time the metric was elided.
  last_elided_timestamp: u64,
  /// The timestamp of the last time the metric was emitted.
  last_emitted_timestamp: u64,
}

impl State {
  const fn new(timestamp: u64, init_step: u64) -> Self {
    Self {
      last_value: None,
      last_type: None,
      last_timestamp: timestamp,
      successive_fails: init_step,
      last_elided_timestamp: 0,
      last_emitted_timestamp: 0,
    }
  }

  fn update(
    &mut self,
    metric: &Metric,
    metric_filters: &[DynamicMetricFilter],
  ) -> (FilterDecision, Option<usize>) {
    let (filter_decision, filter_index, saw_initializing) = metric_filters.iter().enumerate().fold(
      (MetricFilterDecision::NotCovered, None, false),
      |(filter_decision, filter_index, saw_initializing), (i, f)| {
        if filter_decision == MetricFilterDecision::NotCovered
          || filter_decision == MetricFilterDecision::Initializing
        {
          (
            f.decide(metric, &self.last_value, self.last_type),
            Some(i),
            if saw_initializing {
              saw_initializing
            } else {
              filter_decision == MetricFilterDecision::Initializing
            },
          )
        } else {
          (filter_decision, filter_index, saw_initializing)
        }
      },
    );

    self.last_value = Some(metric.value.clone());
    self.last_type = metric.get_id().mtype();
    self.last_timestamp = metric.timestamp;

    let seconds_since_last_emitted = self
      .last_timestamp
      .saturating_sub(self.last_emitted_timestamp);

    match filter_decision {
      MetricFilterDecision::Initializing => (FilterDecision::NotCovered, None),
      MetricFilterDecision::Fail => {
        self.successive_fails += 1;
        (
          FilterDecision::PeriodicOutput(PeriodicOutputState::new(
            self.successive_fails,
            seconds_since_last_emitted,
          )),
          filter_index,
        )
      },
      MetricFilterDecision::Pass => {
        self.successive_fails = 0;
        (FilterDecision::Pass, filter_index)
      },
      MetricFilterDecision::NotCovered => {
        // In the case that any filter was initialized, do not reset successive fails as this will
        // reset the init step jitter.
        // TODO(mattklein123): All of this logic is very confusing and the current filter order
        // is hard coded. This will all have to be redone to support generic/pluggable filtering.
        if !saw_initializing {
          self.successive_fails = 0;
        }
        (FilterDecision::NotCovered, None)
      },
    }
  }

  fn update_last_elided_or_emitted(&mut self, decision: &SkipDecision, timestamp: u64) {
    match decision {
      SkipDecision::Output => self.last_emitted_timestamp = timestamp,
      SkipDecision::DoNotOutput => self.last_elided_timestamp = timestamp,
    }
  }
}

#[derive(Debug, Clone)]
pub struct SkipDecider {
  emit: EmitConfig,
}

impl SkipDecider {
  #[must_use]
  pub const fn new(emit: EmitConfig) -> Self {
    Self { emit }
  }

  #[must_use]
  pub fn decide(
    &self,
    metric: &Metric,
    decision: FilterDecision,
    emit_override: Option<&EmitConfig>,
  ) -> SkipDecision {
    let emit = emit_override.unwrap_or(&self.emit);
    match (emit.emit_type.as_ref().unwrap(), decision) {
      (Emit_type::Ratio(ratio), _) if f64::abs(ratio - 1.0) < f64::EPSILON => SkipDecision::Output,
      (Emit_type::Ratio(ratio), FilterDecision::PeriodicOutput(s)) => {
        let ord = (1.0 / ratio).ceil() as u64;
        if s.successive_fails % ord == 0 {
          SkipDecision::Output
        } else {
          SkipDecision::DoNotOutput
        }
      },
      (Emit_type::Interval(interval), FilterDecision::PeriodicOutput(s)) => {
        if s.seconds_since_last_emitted >= interval.seconds as u64 {
          SkipDecision::Output
        } else {
          SkipDecision::DoNotOutput
        }
      },
      // TODO(mattklein123): We don't need state for the consistent case so should try to remove
      // that as well. This is good enough for now.
      (Emit_type::ConsistentEveryPeriod(config), FilterDecision::PeriodicOutput(_)) => {
        // Use the hash of the name and the metric time stamp to compute a modulo comparison.
        // This will provide consistent output for all tags for a metric name based entirely on
        // the metric timestamp.
        let period_seconds = u64::from(config.period_seconds.unwrap_or(60));
        let normalized_time = metric.timestamp / period_seconds;

        if xxhash_rust::xxh3::xxh3_64(metric.get_id().name()) % u64::from(config.periods)
          == normalized_time % u64::from(config.periods)
        {
          SkipDecision::Output
        } else {
          SkipDecision::DoNotOutput
        }
      },
      (..) => SkipDecision::Output,
    }
  }
}

#[derive(Clone, Debug)]
struct Stats {
  overflow_forward: IntCounter,
  filter_time: Histogram,
}

impl Stats {
  pub fn new(scope: &Scope) -> Self {
    Self {
      overflow_forward: scope.counter("overflow_forward"),
      filter_time: scope.histogram("filter_time"),
    }
  }
}

pub struct ElisionState {
  metric_cache: Arc<MetricCache>,
  state_slot: StateSlotHandle,
  init_step: Arc<AtomicU64>,
  stats: Stats,
}

impl ElisionState {
  #[must_use]
  pub fn new(metric_cache: Arc<MetricCache>, scope: &Scope) -> Self {
    let state_slot = metric_cache.state_slot_handle();
    Self {
      metric_cache,
      state_slot,
      init_step: Arc::new(AtomicU64::new(1)),
      stats: Stats::new(scope),
    }
  }

  pub fn update_and_decide(
    &self,
    metric: &ParsedMetric,
    metric_filters: &[DynamicMetricFilter],
    skip_decider: &SkipDecider,
    emit_override: Option<&EmitConfig>,
  ) -> (SkipDecision, Option<usize>) {
    // Check to see if we had space to cache the incoming metric. If not, we pass it through. We
    // may eventually want this behavior to be configurable.
    let state_slots = match metric.cached_metric() {
      CachedMetric::Loaded(_, state_slots) => state_slots,
      CachedMetric::Overflow => {
        log::debug!("forwarding due to cached metric overflow");
        self.stats.overflow_forward.inc();
        return (SkipDecision::Output, None);
      },
      CachedMetric::NotInitialized => unreachable!("cached metric should be initialized"),
    };

    // TODO(mattklein123): This code was written to assume that "blocked" metrics are actually still
    // emitted on some interval. It's not clear if this is always the way it should be but just
    // noting it here for now. The "init step" is used to jitter the result so we don't emit all
    // the metrics at the same time every interval.
    let state = match state_slots.get_or_init(&self.state_slot, || {
      Mutex::new(State::new(
        metric.metric().timestamp,
        self.init_step.fetch_add(1, Ordering::Relaxed),
      ))
    }) {
      GetOrInitResult::Inserted(inserted) => inserted,
      GetOrInitResult::Existed(existed) => existed,
    };

    let mut state = state.lock();
    let (filter_decision, index) = {
      let _filter_time = self.stats.filter_time.start_timer();
      state.update(metric.metric(), metric_filters)
    };
    let skip_decision = skip_decider.decide(metric.metric(), filter_decision, emit_override);
    state.update_last_elided_or_emitted(&skip_decision, metric.metric().timestamp);
    (skip_decision, index)
  }

  #[must_use]
  pub fn get_last_elided(&self, name: &str) -> u64 {
    // TODO(mattklein123): Currently this is only used for the admin endpoint, and calling this
    // is guaranteed to lock up the server fairly badly if a large number of metrics are being
    // processed. It's unclear why/when this is needed and if there is a better way of doing this.
    let mut last_elided = 0;
    self.metric_cache.iterate(|key, value| {
      if key.name().starts_with(name.as_bytes()) {
        last_elided = value
          .get(&self.state_slot)
          .map_or(last_elided, |state: Arc<Mutex<State>>| {
            max(last_elided, state.lock().last_elided_timestamp)
          });
      }
    });

    last_elided
  }
}
