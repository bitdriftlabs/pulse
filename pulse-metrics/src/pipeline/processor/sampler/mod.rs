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
use crate::pipeline::metric_cache::{CachedMetric, GetOrInitResult, MetricCache, StateSlotHandle};
use crate::pipeline::time::DurationJitter;
use crate::pipeline::PipelineDispatch;
use crate::protos::metric::ParsedMetric;
use async_trait::async_trait;
use bd_server_stats::stats::Scope;
use bd_time::TimeDurationExt;
use parking_lot::Mutex;
use prometheus::IntCounter;
use pulse_common::proto::ProtoDurationToStdDuration;
use pulse_protobuf::protos::pulse::config::processor::v1::sampler::SamplerConfig;
use std::marker::PhantomData;
use std::sync::Arc;
use time::Duration;
use tokio::time::Instant;

const fn default_sampler_emit_interval() -> Duration {
  Duration::hours(12)
}

const fn default_startup_interval() -> Duration {
  Duration::minutes(15)
}

#[derive(Clone, Debug)]
struct Stats {
  overflow_drop: IntCounter,
}

impl Stats {
  pub fn new(scope: &Scope) -> Self {
    Self {
      overflow_drop: scope.counter("overflow_drop"),
    }
  }
}

// A lightweight time-based sampler.
pub(super) struct SamplerProcessor<DurationJitterType> {
  config: SamplerConfig,
  dispatcher: Arc<dyn PipelineDispatch>,
  state: SamplerState<DurationJitterType>,
}

impl<DurationJitterType: DurationJitter> SamplerProcessor<DurationJitterType> {
  pub async fn new(
    config: SamplerConfig,
    context: ProcessorFactoryContext,
  ) -> anyhow::Result<Self> {
    let state = SamplerState::<DurationJitterType>::new(&context.metric_cache, &context.scope);

    Ok(Self {
      config,
      dispatcher: context.dispatcher,
      state,
    })
  }
}

#[async_trait]
impl<DurationJitterType: DurationJitter + Send + Sync> PipelineProcessor
  for SamplerProcessor<DurationJitterType>
{
  async fn recv_samples(self: Arc<Self>, samples: Vec<ParsedMetric>) {
    let now = Instant::now();
    let forward_samples: Vec<ParsedMetric> = samples
      .into_iter()
      .filter(|sample| {
        self.state.update_and_decide(
          now,
          sample,
          self
            .config
            .emit_interval
            .unwrap_duration_or(default_sampler_emit_interval()),
          self
            .config
            .startup_interval
            .unwrap_duration_or(default_startup_interval()),
        ) == SampleDecision::Forward
      })
      .collect();
    if forward_samples.is_empty() {
      return;
    }
    self.dispatcher.send(forward_samples).await;
  }

  async fn start(self: Arc<Self>) {}
}

#[derive(Debug, PartialEq, Eq)]
pub enum SampleDecision {
  Drop,
  Forward,
}

pub struct SamplerState<DurationJitterType> {
  state_slot: StateSlotHandle,
  stats: Stats,
  duration_jitter: PhantomData<DurationJitterType>,
  startup_time: Instant,
}

impl<DurationJitterType: DurationJitter> SamplerState<DurationJitterType> {
  #[must_use]
  pub fn new(metric_cache: &MetricCache, scope: &Scope) -> Self {
    Self {
      state_slot: metric_cache.state_slot_handle(),
      stats: Stats::new(scope),
      duration_jitter: PhantomData,
      startup_time: Instant::now(),
    }
  }

  pub fn update_and_decide(
    &self,
    now: Instant,
    sample: &ParsedMetric,
    emit_interval: Duration,
    startup_interval: Duration,
  ) -> SampleDecision {
    // Drop the sample if the number of cached metrics is above the limit, to avoid overwhelming
    // the outflow.
    // TODO(mattklein123): Eventually we should make this behavior configurable, and hopefully avoid
    // fixed limits but rather allow specifying a max heap and work within that constraint.
    let state_slots = match sample.cached_metric() {
      CachedMetric::Loaded(_, state_slots) => state_slots,
      CachedMetric::Overflow => {
        log::debug!("dropping due to cached metric overflow");
        self.stats.overflow_drop.inc();
        return SampleDecision::Drop;
      },
      CachedMetric::NotInitialized => unreachable!("cached metric should be initialized"),
    };

    match state_slots.get_or_init(&self.state_slot, || {
      if now < startup_interval.add_tokio_instant(self.startup_time) {
        log::trace!("within startup interval, initializing with jitter");
        Mutex::new(
          DurationJitterType::full_jitter_duration(
            (startup_interval.add_tokio_instant(self.startup_time) - now)
              .try_into()
              .unwrap(),
          )
          .add_tokio_instant(now),
        )
      } else {
        log::trace!("post startup interval, initializing now");
        Mutex::new(now)
      }
    }) {
      GetOrInitResult::Existed(existed) | GetOrInitResult::Inserted(existed) => {
        let mut state = existed.lock();
        if now < *state {
          SampleDecision::Drop
        } else {
          *state = DurationJitterType::full_jitter_duration(emit_interval).add_tokio_instant(now);
          SampleDecision::Forward
        }
      },
    }
  }
}
