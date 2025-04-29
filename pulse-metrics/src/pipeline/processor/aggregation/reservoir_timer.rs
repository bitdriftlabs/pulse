// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::make_metric;
use crate::protos::metric::{MetricId, MetricType, MetricValue, ParsedMetric};
use crate::reservoir_timer::ReservoirTimer;
use pulse_common::LossyIntToFloat;
use tokio::time::Instant;

//
// ReservoirTimerAggregation
//

pub(super) struct ReservoirTimerAggregation {
  reservoir: ReservoirTimer,
  emit_as_bulk: bool,
}

impl ReservoirTimerAggregation {
  pub(super) fn new(reservoir_size: u32, emit_as_bulk: bool) -> Self {
    Self {
      reservoir: ReservoirTimer::new(reservoir_size),
      emit_as_bulk,
    }
  }

  pub(super) fn aggregate(&mut self, value: f64, sample_rate: f64) {
    self.reservoir.aggregate(value, sample_rate);
  }

  pub(super) fn produce_metrics(
    &mut self,
    metric_id: &MetricId,
    timestamp: u64,
    now: Instant,
    prom_source: bool,
  ) -> Vec<Option<ParsedMetric>> {
    // Derive the sample rate based on the number of overall samples we got.
    let (reservoir, count) = self.reservoir.drain();
    let sample_rate = reservoir.len().lossy_to_f64() / count;
    if self.emit_as_bulk {
      debug_assert!(!reservoir.is_empty());
      vec![make_metric(
        metric_id.name().clone(),
        metric_id.tags().to_vec(),
        MetricValue::BulkTimer(reservoir),
        Some(sample_rate),
        timestamp,
        now,
        MetricType::BulkTimer,
        prom_source,
      )]
    } else {
      reservoir
        .into_iter()
        .map(|t| {
          make_metric(
            metric_id.name().clone(),
            metric_id.tags().to_vec(),
            MetricValue::Simple(t),
            Some(sample_rate),
            timestamp,
            now,
            MetricType::Timer,
            prom_source,
          )
        })
        .collect()
    }
  }
}
