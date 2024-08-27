// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::cm_quantile::Quantile;
use super::{
  default_aggregation_flush_interval,
  default_aggregation_timer_eps,
  make_metric,
  make_name,
  WrappedConfig,
};
use crate::protos::metric::{CounterType, MetricId, MetricType, MetricValue, ParsedMetric};
use crate::protos::prom::prom_stale_marker;
use pulse_common::proto::ProtoDurationToStdDuration;
use pulse_protobuf::protos::pulse::config::processor::v1::aggregation::AggregationConfig;
use tokio::time::Instant;

// Create an aggregate timer metric.
fn make_timer_metric(
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
      config
        .quantile_timers()
        .prefix
        .as_ref()
        .map_or("", |p| p.as_str()),
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

// Produces metrics for a timer aggregation.
trait TimerProducer {
  fn mean(&self) -> f64;
  fn lower(&self) -> f64;
  fn upper(&self) -> f64;
  fn count(&self) -> f64;
  fn rate(&self, config: &WrappedConfig) -> f64;
  fn quantile(&self, quantile: f64) -> f64;
}

// Produces stale markers.
struct StaleMarkerProducer {}
impl TimerProducer for StaleMarkerProducer {
  fn mean(&self) -> f64 {
    prom_stale_marker()
  }
  fn lower(&self) -> f64 {
    prom_stale_marker()
  }
  fn upper(&self) -> f64 {
    prom_stale_marker()
  }
  fn count(&self) -> f64 {
    prom_stale_marker()
  }
  fn rate(&self, _config: &WrappedConfig) -> f64 {
    prom_stale_marker()
  }
  fn quantile(&self, _quantile: f64) -> f64 {
    prom_stale_marker()
  }
}

// Produces real metrics.
struct RealProducer<'a> {
  aggregation: &'a TimerAggregation,
}

impl TimerProducer for RealProducer<'_> {
  fn mean(&self) -> f64 {
    if self.aggregation.actual_count > 0 {
      self.aggregation.sum / self.aggregation.actual_count as f64
    } else {
      0.0
    }
  }

  fn lower(&self) -> f64 {
    self.aggregation.cm_quantile.min()
  }

  fn upper(&self) -> f64 {
    self.aggregation.cm_quantile.max()
  }

  fn count(&self) -> f64 {
    self.aggregation.count
  }

  fn rate(&self, config: &WrappedConfig) -> f64 {
    self.aggregation.sum
      / config
        .config
        .flush_interval
        .unwrap_duration_or(default_aggregation_flush_interval())
        .as_seconds_f64()
  }

  fn quantile(&self, quantile: f64) -> f64 {
    self.aggregation.cm_quantile.query(quantile)
  }
}

//
// TimerAggregation
//

// An aggregated timer.
pub(super) struct TimerAggregation {
  actual_count: u64,
  count: f64,
  sum: f64,
  cm_quantile: Quantile,
}

impl TimerAggregation {
  pub(super) fn new(config: &AggregationConfig) -> Self {
    Self {
      actual_count: 0,
      count: 0.0,
      sum: 0.0,
      cm_quantile: Quantile::new(
        config
          .quantile_timers()
          .eps
          .unwrap_or(default_aggregation_timer_eps()),
        config.quantile_timers().quantiles.clone(),
      ),
    }
  }

  pub(super) fn aggregate(&mut self, sample: f64, sample_rate: f64) {
    self.actual_count += 1;
    self.count += 1.0 / sample_rate;
    self.sum += sample;
    self.cm_quantile.add_sample(sample);
  }

  fn produce_common(
    metric_id: &MetricId,
    config: &WrappedConfig,
    timestamp: u64,
    now: Instant,
    prom_source: bool,
    producer: &impl TimerProducer,
  ) -> Vec<Option<ParsedMetric>> {
    let mut metrics = vec![
      if config.config.quantile_timers().extended.mean {
        make_timer_metric(
          ".mean",
          metric_id,
          &config.config,
          producer.mean(),
          timestamp,
          now,
          MetricType::Gauge,
          prom_source,
        )
      } else {
        None
      },
      if config.config.quantile_timers().extended.lower {
        make_timer_metric(
          ".lower",
          metric_id,
          &config.config,
          producer.lower(),
          timestamp,
          now,
          MetricType::Gauge,
          prom_source,
        )
      } else {
        None
      },
      if config.config.quantile_timers().extended.upper {
        make_timer_metric(
          ".upper",
          metric_id,
          &config.config,
          producer.upper(),
          timestamp,
          now,
          MetricType::Gauge,
          prom_source,
        )
      } else {
        None
      },
      if config.config.quantile_timers().extended.count {
        make_timer_metric(
          ".count",
          metric_id,
          &config.config,
          producer.count(),
          timestamp,
          now,
          MetricType::Counter(CounterType::Delta),
          prom_source,
        )
      } else {
        None
      },
      if config.config.quantile_timers().extended.rate {
        make_timer_metric(
          ".rate",
          metric_id,
          &config.config,
          producer.rate(config),
          timestamp,
          now,
          MetricType::Counter(CounterType::Delta),
          prom_source,
        )
      } else {
        None
      },
    ];

    for (i, quantile) in config.config.quantile_timers().quantiles.iter().enumerate() {
      metrics.push(make_timer_metric(
        &config.timer_quantile_strings.as_ref().unwrap()[i],
        metric_id,
        &config.config,
        producer.quantile(*quantile),
        timestamp,
        now,
        MetricType::Gauge,
        prom_source,
      ));
    }

    metrics
  }

  pub(super) fn produce_stale_markers(
    metric_id: &MetricId,
    config: &WrappedConfig,
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
    &mut self,
    metric_id: &MetricId,
    config: &WrappedConfig,
    timestamp: u64,
    now: Instant,
    prom_source: bool,
  ) -> Vec<Option<ParsedMetric>> {
    self.cm_quantile.flush();
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
