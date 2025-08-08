// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./mod_test.rs"]
mod mod_test;

mod admin_handler;
mod cm_quantile;
mod counter;
mod gauge;
mod histogram;
mod reservoir_timer;
mod summary;
#[cfg(test)]
mod test;
mod timer;

use self::admin_handler::{LastAggregated, LastAggregatedMetrics};
use self::counter::{AbsoluteCounterAggregation, DeltaCounterAggregation};
use self::gauge::{DirectGaugeAggregation, GaugeAggregation};
use self::histogram::HistogramAggregation;
use self::reservoir_timer::ReservoirTimerAggregation;
use self::summary::SummaryAggregation;
use self::timer::TimerAggregation;
use super::{PipelineProcessor, ProcessorFactoryContext};
use crate::pipeline::PipelineDispatch;
use crate::pipeline::metric_cache::{
  CachedMetric,
  GetOrInitResult,
  MetricCache,
  MetricKey,
  StateSlotHandle,
};
use crate::pipeline::time::{DurationJitter, TimeProvider, next_flush_interval};
use crate::protos::metric::{
  CounterType,
  DownstreamId,
  Metric,
  MetricId,
  MetricSource,
  MetricType,
  MetricValue,
  ParsedMetric,
  TagValue,
  default_timestamp,
};
#[cfg(test)]
use crate::test::thread_synchronizer::ThreadSynchronizer;
use anyhow::bail;
use async_trait::async_trait;
use bd_log::warn_every;
use bd_server_stats::stats::Scope;
use bd_shutdown::ComponentShutdown;
use bd_time::{ProtoDurationExt, TimeDurationExt};
use bytes::{BufMut, Bytes, BytesMut};
use futures::FutureExt;
use parking_lot::{Mutex, RwLock};
use prometheus::{Histogram, IntCounter};
use pulse_common::LossyFloatToInt;
use pulse_common::proto::ProtoDurationToStdDuration;
use pulse_protobuf::protos::pulse::config::processor::v1::aggregation::AggregationConfig;
use pulse_protobuf::protos::pulse::config::processor::v1::aggregation::aggregation_config::{
  QuantileTimers,
  Timer_type,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use time::Duration;
use time::ext::NumericalDuration;
use tokio::time::{Instant, MissedTickBehavior};

const fn default_aggregation_timer_eps() -> f64 {
  0.01
}

fn default_aggregation_quantiles() -> Vec<f64> {
  vec![0.5, 0.95, 0.99]
}

const fn default_aggregation_flush_interval() -> Duration {
  Duration::minutes(1)
}

// TODO(mattklein123): All aggregation types should support emitting either absolute or delta
// counters. Currently the incoming type aggregates to the same outgoing type, but this is not
// correct if enhanced aggregations are being used and the upstream expects absolute counters.

//
// AggregationError
//

#[derive(thiserror::Error, Debug)]
enum AggregationError {
  #[error("Histogram buckets do not match")]
  BucketMismatch,
  #[error("Metric changed type during window")]
  ChangedType,
  #[error("Metric type is unsupported")]
  UnsupportedType,
  #[error("Summary quantiles do not match")]
  QuantileMismatch,
}

type Result<T> = std::result::Result<T, AggregationError>;

//
// Stats
//

struct Stats {
  changed_type: IntCounter,
  unsupported_type: IntCounter,
  overflow: IntCounter,
  histogram_bucket_mismatch: IntCounter,
  summary_quantile_mismatch: IntCounter,
  flush_time: Histogram,
  stale_markers: IntCounter,
}

impl Stats {
  fn new(scope: &Scope) -> Self {
    Self {
      changed_type: scope.counter("changed_type"),
      unsupported_type: scope.counter("unsupported_type"),
      overflow: scope.counter("overflow"),
      histogram_bucket_mismatch: scope.counter("histogram_bucket_mismatch"),
      summary_quantile_mismatch: scope.counter("summary_quantile_mismatch"),
      flush_time: scope.histogram("flush_time"),
      stale_markers: scope.counter("stale_markers"),
    }
  }
}

//
// WrappedConfig
//

struct WrappedConfig {
  config: AggregationConfig,
  timer_quantile_strings: Option<Vec<String>>,
}

impl WrappedConfig {
  // Convert a quantile to a percentile for printing. Inputs already sanitized.
  fn to_percentile(mut quantile: f64) -> String {
    debug_assert!((0.0 ..= 1.0).contains(&quantile));
    quantile *= 100.0;
    #[allow(clippy::while_float)]
    while quantile.fract() != 0.0 {
      quantile *= 10.0;
    }
    format!(".p{}", quantile.lossy_to_u64())
  }

  fn new(mut config: AggregationConfig) -> anyhow::Result<Self> {
    if config.timer_type.is_none() {
      config.timer_type = Some(Timer_type::QuantileTimers(QuantileTimers::default()));
    }

    let timer_quantile_strings =
      if let Some(Timer_type::QuantileTimers(quantile_timers)) = &mut config.timer_type {
        if quantile_timers.quantiles.is_empty() {
          quantile_timers.quantiles = default_aggregation_quantiles();
        }
        if let Some(timer_eps) = quantile_timers.eps {
          // TODO(mattklein123): Do this via PGV.
          if timer_eps <= 0.0 || timer_eps > 0.1 {
            bail!("timer_eps must be > 0.0 and <= 0.1");
          }
        }

        for quantile in &quantile_timers.quantiles {
          // TODO(mattklein123): Do this via PGV.
          if *quantile <= 0.0 || *quantile >= 1.0 {
            bail!("quantiles must be between 0.0 and 1.0");
          }
        }

        Some(
          quantile_timers
            .quantiles
            .iter()
            .map(|q| Self::to_percentile(*q))
            .collect(),
        )
      } else {
        None
      };

    Ok(Self {
      config,
      timer_quantile_strings,
    })
  }
}

//
// AggregationProcessor
//

// The aggregation process replicates Statsite functionality. Currently not all statsite
// functionality is implemented. More functionality will be added on an as needed basis.
pub(super) struct AggregationProcessor {
  config: WrappedConfig,
  stats: Stats,
  dispatcher: Arc<dyn PipelineDispatch>,
  slot_handle: StateSlotHandle,
  metric_cache: Arc<MetricCache>,
  current_snapshot: RwLock<AggregationSnapshot>,
  next_generation: AtomicU64,
  last_aggregated_size: Mutex<Option<usize>>,
  last_aggregated_metrics: LastAggregatedMetrics,
  last_active_metrics: Mutex<Vec<Arc<PerMetricAggregationState>>>,
  _admin_handler_handle: Arc<LastAggregated>,

  #[cfg(test)]
  thread_synchronizer: ThreadSynchronizer,
}

impl AggregationProcessor {
  pub async fn new<JitterType: DurationJitter>(
    config: AggregationConfig,
    context: ProcessorFactoryContext,
  ) -> anyhow::Result<Arc<Self>> {
    let last_aggregated_metrics: LastAggregatedMetrics = Arc::default();

    let processor = Arc::new(Self {
      config: WrappedConfig::new(config)?,
      stats: Stats::new(&context.scope),
      dispatcher: context.dispatcher,
      slot_handle: context.metric_cache.state_slot_handle(),
      metric_cache: context.metric_cache,
      current_snapshot: RwLock::new(AggregationSnapshot::new(1, 0)),
      next_generation: AtomicU64::new(2),
      last_aggregated_size: Mutex::default(),
      last_aggregated_metrics: last_aggregated_metrics.clone(),
      last_active_metrics: Mutex::default(),
      _admin_handler_handle: LastAggregated::get_instance(
        &context.singleton_manager,
        context.admin.as_ref(),
        context.name.clone(),
        last_aggregated_metrics,
      )
      .await,

      #[cfg(test)]
      thread_synchronizer: ThreadSynchronizer::default(),
    });

    let processor_clone = processor.clone();
    let shutdown = context.shutdown_trigger_handle.make_shutdown();
    tokio::spawn(async move {
      processor_clone
        .flush_loop::<JitterType>(shutdown, context.time_provider)
        .await;
    });

    Ok(processor)
  }

  async fn flush_loop<JitterType: DurationJitter>(
    self: &Arc<Self>,
    mut shutdown: ComponentShutdown,
    time_provider: Box<dyn TimeProvider>,
  ) {
    let flush_interval = self
      .config
      .config
      .flush_interval
      .unwrap_duration_or(default_aggregation_flush_interval());
    let mut local_interval = self
      .config
      .config
      .pin_flush_interval_to_wall_clock
      .as_ref()
      .and_then(|b| {
        if b.value {
          None
        } else {
          log::debug!("using local process interval");
          Some(flush_interval.interval_at(MissedTickBehavior::Delay))
        }
      });
    loop {
      let sleep_future = local_interval.as_mut().map_or_else(
        || {
          next_flush_interval(time_provider.as_ref(), flush_interval.whole_seconds())
            .sleep()
            .boxed()
        },
        |local_interval| local_interval.tick().map(|_| ()).boxed(),
      );

      tokio::select! {
        () = sleep_future => {
          self.do_flush::<JitterType>().await;
        }
        () = shutdown.cancelled() => {
          log::debug!("shutting down aggregation task");
          break;
        }
      }
    }

    log::debug!("performing shutdown flush");
    self.do_flush::<JitterType>().await;
    log::debug!("shutdown flush complete");
    drop(shutdown);
  }

  #[allow(clippy::cognitive_complexity)]
  async fn do_flush<JitterType: DurationJitter>(self: &Arc<Self>) {
    let _flush_time = self.stats.flush_time.start_timer();
    log::debug!("performing aggregation flush");

    // Grab the previous snapshot and swap in a new one. First keep track of the size of the
    // metrics so we can pre-allocate the next vector to avoid reallocations when possible.
    // TODO(mattklein123): We could potentially keep the length in an independent atomic to avoid
    // having to lock the metrics vector.
    let previous_capacity = self.current_snapshot.read().active_metrics.lock().len();
    log::debug!("aggregating {previous_capacity} metric(s)");
    let new_snapshot = AggregationSnapshot::new(
      self.next_generation.fetch_add(1, Ordering::Relaxed),
      previous_capacity,
    );
    log::debug!(
      "creating new snapshot with generation {}",
      new_snapshot.generation
    );
    let old_snapshot = {
      let mut current_snapshot = self.current_snapshot.write();
      std::mem::replace(&mut *current_snapshot, new_snapshot)
    };

    #[cfg(test)]
    self.thread_synchronizer.sync_point("do_flush").await;

    let timestamp = default_timestamp();
    let now = Instant::now();
    let active_metrics = old_snapshot.active_metrics.into_inner();
    log::debug!(
      "flushing snapshot with generation {} and {} metrics",
      old_snapshot.generation,
      active_metrics.len()
    );

    let expected_capacity = self
      .last_aggregated_size
      .lock()
      .unwrap_or_else(|| active_metrics.len());
    let mut parsed_metrics = Vec::with_capacity(expected_capacity);
    let mut last_aggregated_keys = if self.config.config.enable_last_aggregation_admin_endpoint {
      Some(Vec::<Arc<MetricKey>>::with_capacity(expected_capacity))
    } else {
      None
    };

    for (i, metric) in active_metrics.iter().enumerate() {
      // TODO(mattklein123): The flush process can take quite some time as it iterates through
      // many thousands of metrics. This is a mild attempt to not completely lock up one of the
      // event loop threads during this process. It's unclear if this is the right number, or if
      // we should be using spawn_blocking() instead, or something else. We can give this a shot
      // and go from there.
      if (i + 1) % 10000 == 0 {
        tokio::task::yield_now().await;
      }

      let metric_id = metric.metric_key.to_metric_id();
      log::debug!("flushing {metric_id}");
      let (mut aggregation, previous_aggregation, prom_source, generation) = {
        let mut locked_state = metric.locked_state.lock();

        // There should always be an aggregation if the metric was added to the aggregation list.
        // In the case where new samples came in before the metric is actually flushed (as this
        // flush process can take a considerable amount of time), those samples are added to the
        // next_aggregation member, so we have a better chance of having samples in every window.
        // Once the flush is complete, we move next_aggregation into aggregation if there is
        // data. Further samples in the window will use aggregation per normal.
        let Some(aggregation) = locked_state.aggregation.take() else {
          panic!(
            "inconsistent aggregation state for metric={} old_snapshot_gen={} \
             current_snapshot_gen={} last_flushed_snapshot_gen={} has_next_aggregation={}",
            metric_id,
            old_snapshot.generation,
            locked_state.current_snapshot_generation,
            locked_state.last_flushed_snapshot_generation,
            locked_state.next_aggregation.is_some()
          );
        };
        locked_state.last_flushed_snapshot_generation = old_snapshot.generation;
        if let Some(next_aggregation) = locked_state.next_aggregation.take() {
          log::debug!("{metric_id} has new data in next_aggregation");
          locked_state.aggregation = Some(next_aggregation);
        }

        (
          aggregation,
          locked_state.previous_aggregation.take(),
          locked_state.prom_source,
          locked_state.current_snapshot_generation,
        )
      };
      let metrics = aggregation.produce_metrics(
        previous_aggregation,
        &metric_id,
        &self.config,
        timestamp,
        now,
        prom_source,
        generation,
      );

      metric.locked_state.lock().previous_aggregation = Some(aggregation);
      self.filter_aggregated_metrics(metrics, &mut parsed_metrics, &mut last_aggregated_keys);
    }

    if self.config.config.emit_prometheus_stale_markers {
      // If we are emitting stale markers we need to see if there are any metrics that were in the
      // last set, but are not in the current generation that is being flushed.
      // TODO(mattklein123): It would be nice to figure out a way of doing this that does not
      // involve iterating over the entire list again, but it's not immediately obvious how to do
      // this efficiently.
      let last_active_metrics = std::mem::take(&mut *self.last_active_metrics.lock());
      for (i, metric) in last_active_metrics.iter().enumerate() {
        // See comment above.
        if (i + 1) % 10000 == 0 {
          tokio::task::yield_now().await;
        }

        if metric.locked_state.lock().current_snapshot_generation < old_snapshot.generation {
          let metric_id = metric.metric_key.to_metric_id();
          log::debug!(
            "{metric_id} is missing from generation {}",
            old_snapshot.generation
          );
          // There must be a previous aggregation if it existed in the previous list.
          let (previous_aggregation, prom_source) = {
            let mut locked_state = metric.locked_state.lock();
            (
              locked_state.previous_aggregation.take().unwrap(),
              locked_state.prom_source,
            )
          };

          let stale_markers = previous_aggregation.produce_stale_markers(
            &metric_id,
            &self.config,
            timestamp,
            now,
            prom_source,
          );

          self
            .stats
            .stale_markers
            .inc_by(self.filter_aggregated_metrics(
              stale_markers,
              &mut parsed_metrics,
              &mut last_aggregated_keys,
            ));
        }
      }

      *self.last_active_metrics.lock() = active_metrics;
    }

    *self.last_aggregated_size.lock() = Some(parsed_metrics.len());
    if let Some(last_aggregated_keys) = last_aggregated_keys {
      *self.last_aggregated_metrics.lock() = Some(last_aggregated_keys);
    }

    log::debug!(
      "flush complete for generation {} with {} metrics",
      old_snapshot.generation,
      parsed_metrics.len()
    );

    if let Some(post_flush_send_jitter) = self.config.config.post_flush_send_jitter.as_ref() {
      let sleep_duration: Duration = post_flush_send_jitter.to_time_duration();
      log::debug!("sleeping for {sleep_duration:?}");
      JitterType::full_jitter_duration(sleep_duration)
        .sleep()
        .await;
    }

    log::debug!("sending {} metrics", parsed_metrics.len());
    self.dispatcher.send(parsed_metrics).await;
  }

  fn filter_aggregated_metrics(
    &self,
    input: Vec<Option<ParsedMetric>>,
    parsed_metrics: &mut Vec<ParsedMetric>,
    last_aggregated_keys: &mut Option<Vec<Arc<MetricKey>>>,
  ) -> u64 {
    let starting_size = parsed_metrics.len();
    let metrics = input.into_iter().filter_map(|mut m| {
      if let Some(metric) = &mut m {
        metric.initialize_cache(&self.metric_cache);
        if let Some(last_aggregated_keys) = last_aggregated_keys
          && let CachedMetric::Loaded(metric_key, _) = metric.cached_metric()
        {
          last_aggregated_keys.push(metric_key);
        }
      }
      m
    });
    parsed_metrics.extend(metrics);
    (parsed_metrics.len() - starting_size).try_into().unwrap()
  }

  fn process_sample(&self, sample: &ParsedMetric) {
    let (metric_key, state_slots) = match sample.cached_metric() {
      CachedMetric::Loaded(metric_key, state_slots) => (metric_key, state_slots),
      CachedMetric::Overflow => {
        self.stats.overflow.inc();
        return;
      },
      CachedMetric::NotInitialized => {
        unreachable!("cached metric should be initialized")
      },
    };

    // Hold the read lock to the snapshot so that we have consistency during flushes. The flusher
    // will acquire a write lock to quickly swap in a new snapshot and then release.
    let current_snapshot = self.current_snapshot.read();

    // Get the per-metric cached aggregation state. Locking this state also serializes the flusher
    // which is iterating through the last generation in the background.
    let per_metric_state = match state_slots.get_or_init(&self.slot_handle, || {
      PerMetricAggregationState::new(metric_key, current_snapshot.generation)
    }) {
      GetOrInitResult::Existed(existed) => existed,
      GetOrInitResult::Inserted(inserted) => inserted,
    };
    let mut locked_per_metric_state = per_metric_state.locked_state.lock();
    match locked_per_metric_state.update(sample, &self.config.config, current_snapshot.generation) {
      Ok(()) => (),
      Err(e) => {
        match e {
          AggregationError::BucketMismatch => self.stats.histogram_bucket_mismatch.inc(),
          AggregationError::ChangedType => self.stats.changed_type.inc(),
          AggregationError::UnsupportedType => self.stats.unsupported_type.inc(),
          AggregationError::QuantileMismatch => self.stats.summary_quantile_mismatch.inc(),
        }

        // TODO(mattklein123): Should we pass these metrics through?
        warn_every!(
          15.seconds(),
          "cannot aggregate metric '{}': {}",
          sample.metric().get_id(),
          e
        );
        return;
      },
    }

    // If the state is not part of the current snapshot generation, add it.
    if locked_per_metric_state.current_snapshot_generation != current_snapshot.generation {
      debug_assert!(locked_per_metric_state.aggregation.is_some());
      log::debug!(
        "adding {} to snapshot generation {}",
        per_metric_state.metric_key.to_metric_id(),
        current_snapshot.generation
      );
      locked_per_metric_state.current_snapshot_generation = current_snapshot.generation;
      let mut active_metrics = current_snapshot.active_metrics.lock();
      debug_assert!(
        !active_metrics
          .iter()
          .any(|metric| metric.metric_key == per_metric_state.metric_key)
      );
      active_metrics.push(per_metric_state.clone());
    }
  }
}

#[async_trait]
impl PipelineProcessor for AggregationProcessor {
  async fn recv_samples(self: Arc<Self>, samples: Vec<ParsedMetric>) {
    for sample in samples {
      self.process_sample(&sample);
    }
  }

  async fn start(self: Arc<Self>) {}
}

fn make_name(prefix: &str, name: &Bytes, postfix: &str) -> Bytes {
  let mut name_buf = BytesMut::with_capacity(prefix.len() + name.len() + postfix.len());
  name_buf.put(prefix.as_bytes());
  name_buf.put(name.as_ref());
  name_buf.put(postfix.as_bytes());
  name_buf.freeze()
}

fn make_metric(
  name: Bytes,
  tags: Vec<TagValue>,
  value: MetricValue,
  sample_rate: Option<f64>,
  timestamp: u64,
  now: Instant,
  metric_type: MetricType,
  prom_source: bool,
) -> Option<ParsedMetric> {
  Some(ParsedMetric::new(
    Metric::new(
      // We assume the tags are already sorted from the input metric. Note this may not always be
      // true if/when we start supporting tag stripping during aggregation.
      MetricId::new(name, Some(metric_type), tags, true)
        .map_err(|e| {
          warn_every!(1.minutes(), "unable to create aggregated metric: {}", e);
        })
        .ok()?,
      sample_rate,
      timestamp,
      value,
    ),
    MetricSource::Aggregation { prom_source },
    now.into_std(),
    DownstreamId::LocalOrigin,
  ))
}

//
// AggregationType
//

// Enum that wraps an individual metric aggregation.
enum AggregationType {
  DeltaCounter(DeltaCounterAggregation),
  AbsoluteCounter(AbsoluteCounterAggregation),
  DeltaGauge(GaugeAggregation),
  DirectGauge(DirectGaugeAggregation),
  Gauge(GaugeAggregation),
  QuantileTimer(TimerAggregation),
  ReservoirTimer(ReservoirTimerAggregation),
  Histogram(HistogramAggregation),
  Summary(SummaryAggregation),
}

impl AggregationType {
  fn produce_metrics(
    &mut self,
    previous_aggregation: Option<Self>,
    metric_id: &MetricId,
    config: &WrappedConfig,
    timestamp: u64,
    now: Instant,
    prom_source: bool,
    generation: u64,
  ) -> Vec<Option<ParsedMetric>> {
    match self {
      Self::DeltaCounter(c) => {
        c.produce_metrics(metric_id, &config.config, timestamp, now, prom_source)
      },
      Self::AbsoluteCounter(c) => c.produce_metrics(
        previous_aggregation.as_ref(),
        metric_id,
        &config.config,
        timestamp,
        now,
        prom_source,
        generation,
      ),
      Self::DeltaGauge(g) | Self::Gauge(g) => {
        g.produce_metrics(metric_id, &config.config, timestamp, now, prom_source)
      },
      Self::DirectGauge(g) => {
        g.produce_metrics(metric_id, &config.config, timestamp, now, prom_source)
      },
      Self::QuantileTimer(t) => t.produce_metrics(metric_id, config, timestamp, now, prom_source),
      Self::ReservoirTimer(r) => r.produce_metrics(metric_id, timestamp, now, prom_source),
      Self::Histogram(h) => h.produce_metrics(
        previous_aggregation,
        metric_id,
        &config.config,
        timestamp,
        now,
        prom_source,
        generation,
      ),
      Self::Summary(s) => s.produce_metrics(metric_id, &config.config, timestamp, now, prom_source),
    }
  }

  fn produce_stale_markers(
    &self,
    metric_id: &MetricId,
    config: &WrappedConfig,
    timestamp: u64,
    now: Instant,
    prom_source: bool,
  ) -> Vec<Option<ParsedMetric>> {
    match self {
      Self::DeltaCounter(_c) => DeltaCounterAggregation::produce_stale_markers(
        metric_id,
        &config.config,
        timestamp,
        now,
        prom_source,
      ),
      Self::AbsoluteCounter(_) => AbsoluteCounterAggregation::produce_stale_markers(
        metric_id,
        &config.config,
        timestamp,
        now,
        prom_source,
      ),
      Self::DeltaGauge(_) | Self::Gauge(_) => GaugeAggregation::produce_stale_markers(
        metric_id,
        &config.config,
        timestamp,
        now,
        prom_source,
      ),
      Self::DirectGauge(_) => DirectGaugeAggregation::produce_stale_markers(
        metric_id,
        &config.config,
        timestamp,
        now,
        prom_source,
      ),
      Self::QuantileTimer(_) => {
        TimerAggregation::produce_stale_markers(metric_id, config, timestamp, now, prom_source)
      },
      Self::ReservoirTimer(_) => {
        // Reservoir timers emit as stasd timers only so stale markers have no real meaning. If
        // applicable allow further pipeline components handle stale markers.
        vec![]
      },
      Self::Histogram(h) => {
        h.produce_stale_markers(metric_id, &config.config, timestamp, now, prom_source)
      },
      Self::Summary(s) => {
        s.produce_stale_markers(metric_id, &config.config, timestamp, now, prom_source)
      },
    }
  }
}

//
// LockedPerMetricAggregationState
//

// This is the portion of PerMetricAggregationState that must be serialized.
struct LockedPerMetricAggregationState {
  // This is the current snapshot generation that the metric has been aggregated for. If this does
  // not equal the current snapshot generation, it will be added to the active list.
  current_snapshot_generation: u64,
  // This is the last snapshot generation that this metric was flushed. This will trail
  // current_snapshot_generation. If the metric has not yet been flushed, data will be added to
  // the next_aggregation bucket. Otherwise it will be added to the aggregation bucket.
  last_flushed_snapshot_generation: u64,
  // Used if a metric has not been flushed yet in the previous generation. This make sure data from
  // the current time window winds up in the next flush.
  next_aggregation: Option<AggregationType>,
  // Used if the metric has been fully flushed in previous generations.
  aggregation: Option<AggregationType>,
  // Data from the last fully flushed aggregation. Not all aggregation types use this.
  previous_aggregation: Option<AggregationType>,
  // Whether the metric came from a prometheus source or not.
  prom_source: bool,
}

impl LockedPerMetricAggregationState {
  fn update_timer(
    config: &AggregationConfig,
    aggregation_to_fill: &mut Option<AggregationType>,
    value: f64,
    sample_rate: Option<f64>,
  ) -> Result<()> {
    match config.timer_type.as_ref().expect("pgv") {
      Timer_type::QuantileTimers(_) => {
        let aggregation = aggregation_to_fill
          .get_or_insert_with(|| AggregationType::QuantileTimer(TimerAggregation::new(config)));
        let AggregationType::QuantileTimer(timer_aggregation) = aggregation else {
          return Err(AggregationError::ChangedType);
        };
        timer_aggregation.aggregate(value, sample_rate.unwrap_or(1.0));
      },
      Timer_type::ReservoirTimers(r) => {
        let aggregation = aggregation_to_fill.get_or_insert_with(|| {
          AggregationType::ReservoirTimer(ReservoirTimerAggregation::new(
            r.reservoir_size.unwrap_or(100),
            r.emit_as_bulk_timer,
          ))
        });
        let AggregationType::ReservoirTimer(timer_aggregation) = aggregation else {
          return Err(AggregationError::ChangedType);
        };
        timer_aggregation.aggregate(value, sample_rate.unwrap_or(1.0));
      },
    }
    Ok(())
  }

  fn update(
    &mut self,
    sample: &ParsedMetric,
    config: &AggregationConfig,
    generation: u64,
  ) -> Result<()> {
    // Decide which aggregation bucket to use. If the last flushed generation is 1 behind the
    // current generation this metric is fully flushed, so we use the primary bucket. Otherwise
    // we use the next_aggregation bucket, unless the current primary aggregation is empty. This
    // can happen if this metric missed previous sample windows.
    if self.aggregation.is_none() {
      // If there is no current aggregation, make sure we are caught up in case we missed a window.
      self.last_flushed_snapshot_generation = generation - 1;
    }

    let aggregation_to_fill = if self.last_flushed_snapshot_generation + 1 == generation {
      &mut self.aggregation
    } else {
      log::debug!(
        "{} still pending flush, using next aggregation storage {} {}",
        sample.metric().get_id(),
        self.last_flushed_snapshot_generation,
        generation,
      );
      &mut self.next_aggregation
    };

    // Last metric wins for prom source. This is not necessarily true if someone sets up a pipeline
    // that funnels multiple sources in that map to the same name, but we will ignore that for right
    // now.
    self.prom_source = matches!(sample.source(), MetricSource::PromRemoteWrite);

    // Convert a gauge to a direct gauge if we are not doing extended gauge aggregations.
    let metric_type = sample.metric().get_id().mtype();
    let metric_type =
      if matches!(metric_type, Some(MetricType::Gauge)) && !config.gauges.extended.is_some() {
        Some(MetricType::DirectGauge)
      } else {
        metric_type
      };

    #[allow(clippy::match_same_arms)]
    match metric_type {
      None => {
        // If there is no type (e.g., from Carbon), there is no way we can do aggregation.
        return Err(AggregationError::UnsupportedType);
      },
      Some(MetricType::Counter(CounterType::Delta)) => {
        let aggregation = aggregation_to_fill
          .get_or_insert_with(|| AggregationType::DeltaCounter(DeltaCounterAggregation::default()));
        let AggregationType::DeltaCounter(counter_aggregation) = aggregation else {
          return Err(AggregationError::ChangedType);
        };
        counter_aggregation.aggregate(
          sample.metric().value.to_simple(),
          sample.metric().sample_rate.unwrap_or(1.0),
        );
      },
      Some(MetricType::Counter(CounterType::Absolute)) => {
        let aggregation = aggregation_to_fill.get_or_insert_with(|| {
          AggregationType::AbsoluteCounter(AbsoluteCounterAggregation::default())
        });
        let AggregationType::AbsoluteCounter(counter_aggregation) = aggregation else {
          return Err(AggregationError::ChangedType);
        };
        counter_aggregation.aggregate(
          sample.metric().value.to_simple(),
          sample.metric(),
          sample.downstream_id(),
        );
      },
      Some(MetricType::Gauge) => {
        let aggregation = aggregation_to_fill
          .get_or_insert_with(|| AggregationType::Gauge(GaugeAggregation::default()));
        let AggregationType::Gauge(gauge_aggregation) = aggregation else {
          return Err(AggregationError::ChangedType);
        };
        gauge_aggregation.aggregate(
          sample.metric().value.to_simple(),
          false,
          sample.metric().get_id(),
          sample.downstream_id(),
        );
      },
      Some(MetricType::DeltaGauge) => {
        let aggregation = aggregation_to_fill
          .get_or_insert_with(|| AggregationType::DeltaGauge(GaugeAggregation::default()));
        let AggregationType::DeltaGauge(gauge_aggregation) = aggregation else {
          return Err(AggregationError::ChangedType);
        };
        gauge_aggregation.aggregate(
          sample.metric().value.to_simple(),
          true,
          sample.metric().get_id(),
          sample.downstream_id(),
        );
      },
      Some(MetricType::DirectGauge) => {
        let aggregation = aggregation_to_fill
          .get_or_insert_with(|| AggregationType::DirectGauge(DirectGaugeAggregation::default()));
        let AggregationType::DirectGauge(gauge_aggregation) = aggregation else {
          return Err(AggregationError::ChangedType);
        };
        gauge_aggregation.aggregate(sample.metric().value.to_simple());
      },
      Some(MetricType::Histogram) => {
        let aggregation = aggregation_to_fill
          .get_or_insert_with(|| AggregationType::Histogram(HistogramAggregation::default()));
        let AggregationType::Histogram(histogram_aggregation) = aggregation else {
          return Err(AggregationError::ChangedType);
        };

        histogram_aggregation.aggregate(sample.metric(), sample.downstream_id())?;
      },
      Some(MetricType::BulkTimer) => {
        // Empty is blocked at inflow.
        debug_assert!(!sample.metric().value.to_bulk_timer().is_empty());
        for value in sample.metric().value.to_bulk_timer() {
          Self::update_timer(
            config,
            aggregation_to_fill,
            *value,
            sample.metric().sample_rate,
          )?;
        }
      },
      Some(MetricType::Timer) => Self::update_timer(
        config,
        aggregation_to_fill,
        sample.metric().value.to_simple(),
        sample.metric().sample_rate,
      )?,
      Some(MetricType::Summary) => {
        let aggregation = aggregation_to_fill
          .get_or_insert_with(|| AggregationType::Summary(SummaryAggregation::default()));
        let AggregationType::Summary(summary_aggregation) = aggregation else {
          return Err(AggregationError::ChangedType);
        };

        summary_aggregation.aggregate(sample.metric().value.to_summary())?;
      },
    }

    Ok(())
  }
}

//
// PerMetricAggregationState
//

// This structure is stored inside the metric cache on a per metric basis and is used for
// aggregation as data is received.
struct PerMetricAggregationState {
  metric_key: Arc<MetricKey>,
  locked_state: Mutex<LockedPerMetricAggregationState>,
}

impl PerMetricAggregationState {
  const fn new(metric_key: Arc<MetricKey>, generation: u64) -> Self {
    Self {
      metric_key,
      locked_state: Mutex::new(LockedPerMetricAggregationState {
        current_snapshot_generation: generation - 1,
        last_flushed_snapshot_generation: generation - 1,
        next_aggregation: None,
        aggregation: None,
        previous_aggregation: None,
        prom_source: false,
      }),
    }
  }
}

//
// AggregationSnapshot
//

// A snapshot of metrics in a given aggregation window. At aggregation time, all metrics will be
// finalized and flushed to further pipeline stages.
struct AggregationSnapshot {
  generation: u64,
  active_metrics: Mutex<Vec<Arc<PerMetricAggregationState>>>,
}

impl AggregationSnapshot {
  fn new(generation: u64, capacity: usize) -> Self {
    Self {
      generation,
      active_metrics: Mutex::new(Vec::with_capacity(capacity)),
    }
  }
}
