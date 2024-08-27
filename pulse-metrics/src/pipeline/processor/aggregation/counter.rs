// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./counter_test.rs"]
mod counter_test;

use super::AggregationType;
use crate::pipeline::processor::aggregation::{
  default_aggregation_flush_interval,
  make_metric,
  make_name,
};
use crate::protos::metric::{
  CounterType,
  DownstreamId,
  Metric,
  MetricId,
  MetricType,
  MetricValue,
  ParsedMetric,
};
use crate::protos::prom::prom_stale_marker;
use bd_log::warn_every;
use hashbrown::HashMap;
use log::Level;
use pulse_common::proto::ProtoDurationToStdDuration;
use pulse_protobuf::protos::pulse::config::processor::v1::aggregation::AggregationConfig;
use time::ext::NumericalDuration;
use tokio::time::Instant;

// Create an aggregate counter metric.
fn make_counter_metric(
  postfix: &str,
  metric_id: &MetricId,
  config: &AggregationConfig,
  value: f64,
  timestamp: u64,
  now: Instant,
  metric_type: MetricType,
  prom_source: bool,
) -> Option<ParsedMetric> {
  make_metric(
    make_name(
      config.counters.prefix.as_ref().map_or("", |p| p.as_str()),
      metric_id.name(),
      postfix,
    ),
    metric_id.tags().to_vec(),
    MetricValue::Simple(value),
    None,
    timestamp,
    now,
    metric_type,
    prom_source,
  )
}

//
// CurrentValue
//

// Current tracked value for an individual DownstreamId within an absolute counter aggregation.
#[derive(Default)]
struct CurrentValue {
  last_absolute_value: Option<f64>,
}

// TODO(mattklein123): Make this into an admin endpoint so we can turn this off and on.
const DEBUG_METRICS: &[&[u8]] = &[];

//
// AbsoluteCounterAggregation
//

// An aggregation for an absolute counter. Absolute counters are challenging to correctly aggregate
// due to the fact that "last value" is not a sufficient signal when collapsing data from multiple
// disparate sources. Instead, we need to track each source and compute deltas for each source.
// These deltas may span across aggregation intervals. We also have to track reset across sources.
// See below for detailed comments on the various pieces of the algorithm.
#[derive(Default)]
pub(super) struct AbsoluteCounterAggregation {
  sources: HashMap<DownstreamId, CurrentValue, ahash::RandomState>,
  last_emitted: Option<f64>,
}

impl AbsoluteCounterAggregation {
  pub(super) fn aggregate(&mut self, sample: f64, metric: &Metric, downstream_id: &DownstreamId) {
    let level = if DEBUG_METRICS.contains(&metric.get_id().name().as_ref()) {
      Level::Info
    } else {
      Level::Trace
    };

    // Absolute counters should never be below zero. Just drop.
    if sample < 0.0 {
      warn_every!(
        1.minutes(),
        "{}: sample below zero, dropping",
        metric.get_id().to_string()
      );
      return;
    }

    // Similar to MetricId/MetricKey cases, DownstreamId can hold references to large network
    // payloads. We must create an owned copy before insertion. Using hashbrown directly avoids
    // key creation unless necessary.
    let (_, current_value) = self
      .sources
      .raw_entry_mut()
      .from_key(downstream_id)
      .or_insert_with(|| (downstream_id.clone_for_hash_key(), CurrentValue::default()));

    log::log!(
      level,
      "{}/{:?} updating last absolute value: {}",
      metric.get_id(),
      downstream_id,
      sample
    );
    current_value.last_absolute_value = Some(sample);
  }

  // Attempt to determine the delta using the data from the previous aggregation window.
  fn find_delta_using_previous(
    previous: Option<&Self>,
    downstream_id: &DownstreamId,
    current_value: &CurrentValue,
  ) -> Option<f64> {
    // If there is no previous aggregation, or the aggregation changed type, there is nothing we
    // can do.
    let previous = previous?;

    // We can now compute the delta using the data from the previous window. We also have to handle
    // reset across the window.
    previous.sources.get(downstream_id).map(|previous_value| {
      if current_value.last_absolute_value.unwrap() < previous_value.last_absolute_value.unwrap() {
        log::trace!("detected sample reset across interval");
        current_value.last_absolute_value.unwrap()
      } else {
        current_value.last_absolute_value.unwrap() - previous_value.last_absolute_value.unwrap()
      }
    })
  }

  pub(super) fn produce_value(
    &mut self,
    previous_aggregation: Option<&Self>,
    metric_id: &MetricId,
    config: &AggregationConfig,
    generation: u64,
  ) -> Option<(f64, CounterType)> {
    let level = if DEBUG_METRICS.contains(&metric_id.name().as_ref()) {
      Level::Info
    } else {
      Level::Trace
    };

    let mut delta = None;
    for (downstream_id, current_value) in &self.sources {
      // For every value we need to have a value from the previous cycle. This lets us know that we
      // can compute an accurate delta if we just restarted. The exception to this is after we have
      // done some number of flushes, we can assume that if we see a new source, it's actually a
      // restart, and in that case we can take it as a delta. If we don't do this and sources are
      // constantly coming and going we will never get a value. This is obviously highly imperfect
      // but we can see how it goes.
      //
      // Note: The initial implementation tried to keep a delta within the window so that we could
      // emit on first flush, but it turns out that we must in general always use the previous
      // flush counter to avoid missing data. Thus, the delta only has value on startup, which seems
      // not worth it so it has been removed.
      //
      // TODO(mattklein123): This will only consider data from the most recent window. If we drop
      // samples temporarily from some sources, this is going to look like new data / reset. Should
      // we actually keep previous data for multiple windows?
      if let Some(current_delta) =
        Self::find_delta_using_previous(previous_aggregation, downstream_id, current_value)
      {
        // This branch means we are able to grab the delta using data from the previous window.
        log::log!(
          level,
          "computing delta via previous interval for {}/{:?}",
          metric_id,
          downstream_id
        );
        *delta.get_or_insert(0.0) += current_delta;
      } else if generation > 2 {
        // This branch means we are after our second flush interval, so we are assuming a new source
        // is actually new, and we take the absolute value as a reset. We start this at gen 3 as
        // with flush intervals pegged to wall clock time, the first flush interval may be short
        // so we need to wait 2 to be reasonably sure we don't have any data.
        // TODO(mattklein123): Per above, it's unclear if this is the best thing to do and we may
        // have to evolve this over time.
        log::log!(
          level,
          "assuming new source for {}/{:?}",
          metric_id,
          downstream_id
        );
        *delta.get_or_insert(0.0) += current_value.last_absolute_value.unwrap();
      } else {
        log::log!(
          level,
          "{}/{:?} first flush interval, cannot emit",
          metric_id,
          downstream_id
        );
        return None;
      }
    }

    let delta = delta?;

    // See if we can pull last emitted from the previous window. Use this to create a new absolute
    // value given the current delta.
    let last_emitted = previous_aggregation.and_then(|c| c.last_emitted);
    self.last_emitted = Some(last_emitted.unwrap_or_default() + delta);
    Some(
      if config.counters.absolute_counters.emit_as_delta_rate {
        let rate = delta
          / config
            .flush_interval
            .unwrap_duration_or(default_aggregation_flush_interval())
            .as_seconds_f64();
        log::log!(level, "emitting rate={rate} for {}", metric_id);
        (rate, CounterType::Delta)
      } else {
        log::log!(
          level,
          "emitting absolute={} for {}",
          self.last_emitted.unwrap(),
          metric_id
        );
        (self.last_emitted.unwrap(), CounterType::Absolute)
      },
    )
  }

  pub(super) fn produce_stale_markers(
    metric_id: &MetricId,
    config: &AggregationConfig,
    timestamp: u64,
    now: Instant,
    prom_source: bool,
  ) -> Vec<Option<ParsedMetric>> {
    vec![make_counter_metric(
      "",
      metric_id,
      config,
      prom_stale_marker(),
      timestamp,
      now,
      MetricType::Counter(
        if config.counters.absolute_counters.emit_as_delta_rate {
          CounterType::Delta
        } else {
          CounterType::Absolute
        },
      ),
      prom_source,
    )]
  }

  pub(super) fn produce_metrics(
    &mut self,
    previous_aggregation: Option<&AggregationType>,
    metric_id: &MetricId,
    config: &AggregationConfig,
    timestamp: u64,
    now: Instant,
    prom_source: bool,
    generation: u64,
  ) -> Vec<Option<ParsedMetric>> {
    if let Some((value_to_emit, counter_type)) = self.produce_value(
      previous_aggregation.as_ref().and_then(|p| {
        if let AggregationType::AbsoluteCounter(c) = p {
          Some(c)
        } else {
          None
        }
      }),
      metric_id,
      config,
      generation,
    ) {
      vec![make_counter_metric(
        "",
        metric_id,
        config,
        value_to_emit,
        timestamp,
        now,
        MetricType::Counter(counter_type),
        prom_source,
      )]
    } else {
      vec![]
    }
  }
}

// Produces metrics for a delta counter.
trait DeltaCounterProducer {
  fn count(&self) -> f64;
  fn sum(&self) -> f64;
  fn lower(&self) -> f64;
  fn upper(&self) -> f64;
  fn rate(&self, config: &AggregationConfig) -> f64;
}

// Produces stale markers.
struct StaleMarkerProducer {}
impl DeltaCounterProducer for StaleMarkerProducer {
  fn count(&self) -> f64 {
    prom_stale_marker()
  }
  fn sum(&self) -> f64 {
    prom_stale_marker()
  }
  fn lower(&self) -> f64 {
    prom_stale_marker()
  }
  fn upper(&self) -> f64 {
    prom_stale_marker()
  }
  fn rate(&self, _config: &AggregationConfig) -> f64 {
    prom_stale_marker()
  }
}

// Produces real metrics.
struct RealProducer<'a> {
  aggregation: &'a DeltaCounterAggregation,
}
impl DeltaCounterProducer for RealProducer<'_> {
  fn count(&self) -> f64 {
    self.aggregation.count as f64
  }
  fn sum(&self) -> f64 {
    self.aggregation.sum
  }
  fn lower(&self) -> f64 {
    self.aggregation.min
  }
  fn upper(&self) -> f64 {
    self.aggregation.max
  }
  fn rate(&self, config: &AggregationConfig) -> f64 {
    self.aggregation.sum
      / config
        .flush_interval
        .unwrap_duration_or(default_aggregation_flush_interval())
        .as_seconds_f64()
  }
}

//
// DeltaCounterAggregation
//

// An aggregated counter.
#[derive(Default)]
pub(super) struct DeltaCounterAggregation {
  count: u64,
  sum: f64,
  min: f64,
  max: f64,
}

impl DeltaCounterAggregation {
  pub(super) fn aggregate(&mut self, sample: f64, sample_rate: f64) {
    if sample == 0.0 {
      // If the counter sample is 0, just ignore it. This is different from statsite, but in
      // practice it should not matter, and it prevents the count from going up which will confuse
      // zero elision.
      return;
    }

    let sample = sample * (1.0 / sample_rate);
    if self.count == 0 {
      self.min = sample;
      self.max = sample;
    } else {
      if self.min > sample {
        self.min = sample;
      }
      if self.max < sample {
        self.max = sample;
      }
    }
    self.count += (1.0 / sample_rate) as u64;
    self.sum += sample;
  }

  fn produce_common(
    metric_id: &MetricId,
    config: &AggregationConfig,
    timestamp: u64,
    now: Instant,
    prom_source: bool,
    producer: &impl DeltaCounterProducer,
  ) -> Vec<Option<ParsedMetric>> {
    if config.counters.extended.is_some() {
      vec![
        if config.counters.extended.count {
          make_counter_metric(
            ".count",
            metric_id,
            config,
            producer.count(),
            timestamp,
            now,
            MetricType::Counter(CounterType::Delta),
            prom_source,
          )
        } else {
          None
        },
        if config.counters.extended.sum {
          make_counter_metric(
            ".sum",
            metric_id,
            config,
            producer.sum(),
            timestamp,
            now,
            MetricType::Counter(CounterType::Delta),
            prom_source,
          )
        } else {
          None
        },
        if config.counters.extended.lower {
          make_counter_metric(
            ".lower",
            metric_id,
            config,
            producer.lower(),
            timestamp,
            now,
            MetricType::Gauge,
            prom_source,
          )
        } else {
          None
        },
        if config.counters.extended.upper {
          make_counter_metric(
            ".upper",
            metric_id,
            config,
            producer.upper(),
            timestamp,
            now,
            MetricType::Gauge,
            prom_source,
          )
        } else {
          None
        },
        if config.counters.extended.rate {
          make_counter_metric(
            ".rate",
            metric_id,
            config,
            producer.rate(config),
            timestamp,
            now,
            MetricType::Counter(CounterType::Delta),
            prom_source,
          )
        } else {
          None
        },
      ]
    } else {
      vec![make_counter_metric(
        "",
        metric_id,
        config,
        producer.sum(),
        timestamp,
        now,
        MetricType::Counter(CounterType::Delta),
        prom_source,
      )]
    }
  }

  pub(super) fn produce_stale_markers(
    metric_id: &MetricId,
    config: &AggregationConfig,
    timestamp: u64,
    now: Instant,
    prom_source: bool,
  ) -> Vec<Option<ParsedMetric>> {
    Self::produce_common(
      metric_id,
      config,
      timestamp,
      now,
      prom_source,
      &StaleMarkerProducer {},
    )
  }

  pub(super) fn produce_metrics(
    &self,
    metric_id: &MetricId,
    config: &AggregationConfig,
    timestamp: u64,
    now: Instant,
    prom_source: bool,
  ) -> Vec<Option<ParsedMetric>> {
    Self::produce_common(
      metric_id,
      config,
      timestamp,
      now,
      prom_source,
      &RealProducer { aggregation: self },
    )
  }
}
