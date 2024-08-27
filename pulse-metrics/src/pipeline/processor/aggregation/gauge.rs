// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::{make_metric, make_name};
use crate::protos::metric::{MetricId, MetricType, MetricValue, ParsedMetric};
use crate::protos::prom::prom_stale_marker;
use pulse_protobuf::protos::pulse::config::processor::v1::aggregation::AggregationConfig;
use tokio::time::Instant;

// Create an aggregate gauge metric.
fn make_gauge_metric(
  postfix: &str,
  metric_id: &MetricId,
  config: &AggregationConfig,
  value: f64,
  timestamp: u64,
  now: Instant,
  prom_source: bool,
) -> Option<ParsedMetric> {
  make_metric(
    make_name(
      config.gauges.prefix.as_ref().map_or("", |p| p.as_str()),
      metric_id.name(),
      postfix,
    ),
    metric_id.tags().to_vec(),
    MetricValue::Simple(value),
    None,
    timestamp,
    now,
    MetricType::Gauge,
    prom_source,
  )
}

//
// DirectGaugeAggregation
//

// An aggregated direct gauge.
#[derive(Default)]
pub(super) struct DirectGaugeAggregation {
  value: f64,
}

impl DirectGaugeAggregation {
  pub(super) fn aggregate(&mut self, sample: f64) {
    self.value = sample;
  }

  pub(super) fn produce_stale_markers(
    metric_id: &MetricId,
    config: &AggregationConfig,
    timestamp: u64,
    now: Instant,
    prom_source: bool,
  ) -> Vec<Option<ParsedMetric>> {
    vec![make_gauge_metric(
      "",
      metric_id,
      config,
      prom_stale_marker(),
      timestamp,
      now,
      prom_source,
    )]
  }

  pub(super) fn produce_metrics(
    &self,
    metric_id: &MetricId,
    config: &AggregationConfig,
    timestamp: u64,
    now: Instant,
    prom_source: bool,
  ) -> Vec<Option<ParsedMetric>> {
    vec![make_gauge_metric(
      "",
      metric_id,
      config,
      self.value,
      timestamp,
      now,
      prom_source,
    )]
  }
}

// Produces metrics for a gauge aggregation.
trait GaugeProducer {
  fn value(&self) -> f64;
  fn sum(&self) -> f64;
  fn mean(&self) -> f64;
  fn min(&self) -> f64;
  fn max(&self) -> f64;
}

// Produces stale markers.
struct StaleMarkerProducer {}
impl GaugeProducer for StaleMarkerProducer {
  fn value(&self) -> f64 {
    prom_stale_marker()
  }
  fn sum(&self) -> f64 {
    prom_stale_marker()
  }
  fn mean(&self) -> f64 {
    prom_stale_marker()
  }
  fn min(&self) -> f64 {
    prom_stale_marker()
  }
  fn max(&self) -> f64 {
    prom_stale_marker()
  }
}

// Produces real metrics.
struct RealProducer<'a> {
  aggregation: &'a GaugeAggregation,
}
impl GaugeProducer for RealProducer<'_> {
  fn value(&self) -> f64 {
    self.aggregation.value
  }
  fn sum(&self) -> f64 {
    self.aggregation.sum
  }
  fn mean(&self) -> f64 {
    if self.aggregation.count > 0 {
      self.aggregation.sum / self.aggregation.count as f64
    } else {
      0.0
    }
  }
  fn min(&self) -> f64 {
    self.aggregation.min
  }
  fn max(&self) -> f64 {
    self.aggregation.max
  }
}

//
// GaugeAggregation
//

// An aggregated gauge.
#[derive(Default)]
pub(super) struct GaugeAggregation {
  count: u64,
  sum: f64,
  value: f64,
  min: f64,
  max: f64,
}

impl GaugeAggregation {
  pub(super) fn aggregate(&mut self, sample: f64, delta: bool) {
    if delta {
      self.value += sample;
    } else {
      self.value = sample;
    }

    if self.count == 0 {
      self.min = sample;
      self.max = sample;
    } else if self.min > sample {
      self.min = sample;
    } else if self.max < sample {
      self.max = sample;
    }

    self.sum += sample;
    self.count += 1;
  }

  fn produce_common(
    metric_id: &MetricId,
    config: &AggregationConfig,
    timestamp: u64,
    now: Instant,
    prom_source: bool,
    producer: &impl GaugeProducer,
  ) -> Vec<Option<ParsedMetric>> {
    vec![
      make_gauge_metric(
        "",
        metric_id,
        config,
        producer.value(),
        timestamp,
        now,
        prom_source,
      ),
      if config.gauges.extended.sum {
        make_gauge_metric(
          ".sum",
          metric_id,
          config,
          producer.sum(),
          timestamp,
          now,
          prom_source,
        )
      } else {
        None
      },
      if config.gauges.extended.mean {
        make_gauge_metric(
          ".mean",
          metric_id,
          config,
          producer.mean(),
          timestamp,
          now,
          prom_source,
        )
      } else {
        None
      },
      if config.gauges.extended.min {
        make_gauge_metric(
          ".min",
          metric_id,
          config,
          producer.min(),
          timestamp,
          now,
          prom_source,
        )
      } else {
        None
      },
      if config.gauges.extended.max {
        make_gauge_metric(
          ".max",
          metric_id,
          config,
          producer.max(),
          timestamp,
          now,
          prom_source,
        )
      } else {
        None
      },
    ]
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
