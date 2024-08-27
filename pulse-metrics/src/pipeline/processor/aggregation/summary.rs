// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::AggregationError;
use crate::pipeline::processor::aggregation::{make_metric, make_name};
use crate::protos::metric::{
  MetricId,
  MetricType,
  MetricValue,
  ParsedMetric,
  SummaryBucket,
  SummaryData,
};
use crate::protos::prom::prom_stale_marker;
use pulse_protobuf::protos::pulse::config::processor::v1::aggregation::AggregationConfig;
use tokio::time::Instant;

// TODO(mattklein123): Prometheus style summaries use absolute counters, and should use absolute
// style aggregation similar to counters. Come back to this once that code is stabilized.

#[derive(Default)]
pub(super) struct SummaryAggregation {
  data: SummaryData,
}

impl SummaryAggregation {
  pub(super) fn aggregate(&mut self, data: &SummaryData) -> Result<(), AggregationError> {
    // First sample wins the quantiles for the interval. Subsequent quantiles must match.
    if self.data.quantiles.is_empty() {
      self.data = data.clone();
    } else {
      // TODO(mattklein123): Currently we just stomp the quantiles and add the count/sum. We should
      // do something better here like at least take the average of each quantile?
      if self.data.quantiles.len() != data.quantiles.len() {
        return Err(AggregationError::QuantileMismatch);
      }
      for (our, their) in self.data.quantiles.iter().zip(&data.quantiles) {
        if our.quantile != their.quantile {
          return Err(AggregationError::QuantileMismatch);
        }
      }
      for (our, their) in self.data.quantiles.iter_mut().zip(&data.quantiles) {
        our.value = their.value;
      }
      self.data.sample_count += data.sample_count;
      self.data.sample_sum += data.sample_sum;
    }

    Ok(())
  }

  pub(super) fn produce_stale_markers(
    &self,
    metric_id: &MetricId,
    config: &AggregationConfig,
    timestamp: u64,
    now: Instant,
    prom_source: bool,
  ) -> Vec<Option<ParsedMetric>> {
    vec![make_metric(
      make_name(
        config.summaries.prefix.as_ref().map_or("", |p| p.as_str()),
        metric_id.name(),
        "",
      ),
      metric_id.tags().to_vec(),
      MetricValue::Summary(SummaryData {
        quantiles: self
          .data
          .quantiles
          .iter()
          .map(|q| SummaryBucket {
            quantile: q.quantile,
            value: prom_stale_marker(),
          })
          .collect(),
        sample_count: prom_stale_marker(),
        sample_sum: prom_stale_marker(),
      }),
      None,
      timestamp,
      now,
      MetricType::Summary,
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
    vec![make_metric(
      make_name(
        config.summaries.prefix.as_ref().map_or("", |p| p.as_str()),
        metric_id.name(),
        "",
      ),
      metric_id.tags().to_vec(),
      MetricValue::Summary(self.data.clone()),
      None,
      timestamp,
      now,
      MetricType::Summary,
      prom_source,
    )]
  }
}
