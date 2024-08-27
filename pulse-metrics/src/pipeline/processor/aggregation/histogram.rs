// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./histogram_test.rs"]
mod histogram_test;

use super::counter::AbsoluteCounterAggregation;
use super::{AggregationError, AggregationType};
use crate::pipeline::processor::aggregation::{make_metric, make_name};
use crate::protos::metric::{
  DownstreamId,
  HistogramBucket,
  HistogramData,
  Metric,
  MetricId,
  MetricType,
  MetricValue,
  ParsedMetric,
};
use crate::protos::prom::prom_stale_marker;
use anyhow::bail;
use itertools::Itertools;
use pulse_protobuf::protos::pulse::config::processor::v1::aggregation::AggregationConfig;
use tokio::time::Instant;

//
// HistogramAggregation
//

// Performs aggregation for a histogram. This assumes histogram counts are absolute counters.
//
// TODO(mattklein123): This implementation uses a HashMap for each count to keep track of sources.
// It would more efficient to invert it and only use a single HashMap. This structure makes it easy
// to share the code with the counter implementation so we will start with that and can improve
// later.
#[derive(Default)]
pub(super) struct HistogramAggregation {
  buckets: Vec<(AbsoluteCounterAggregation, f64)>,
  sample_count: AbsoluteCounterAggregation,
  sample_sum: AbsoluteCounterAggregation,
}

impl HistogramAggregation {
  pub(super) fn aggregate(
    &mut self,
    metric: &Metric,
    downstream_id: &DownstreamId,
  ) -> Result<(), AggregationError> {
    let data = metric.value.to_histogram();

    // Currently the first sample sets the buckets for the interval. For server, this is probably
    // OK as we expect all senders to use the same buckets within reason.
    // TODO(mattklein123): loop-api has bucket merging code which we could use here if/when we want
    // to improve this, but we can start here just to get something working.
    if self.buckets.is_empty() {
      self.buckets = data
        .buckets
        .iter()
        .map(|b| (AbsoluteCounterAggregation::default(), b.le))
        .collect();
    }

    if self.buckets.len() != data.buckets.len() {
      return Err(AggregationError::BucketMismatch);
    }
    for ((_, our_le), their) in self.buckets.iter().zip(&data.buckets) {
      if *our_le != their.le {
        return Err(AggregationError::BucketMismatch);
      }
    }
    for ((our_count, _), their) in self.buckets.iter_mut().zip(data.buckets.iter()) {
      our_count.aggregate(their.count, metric, downstream_id);
    }
    self
      .sample_count
      .aggregate(data.sample_count, metric, downstream_id);
    self
      .sample_sum
      .aggregate(data.sample_sum, metric, downstream_id);

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
        config.histograms.prefix.as_ref().map_or("", |p| p.as_str()),
        metric_id.name(),
        "",
      ),
      metric_id.tags().to_vec(),
      MetricValue::Histogram(HistogramData {
        buckets: self
          .buckets
          .iter()
          .map(|(_, le)| HistogramBucket {
            count: prom_stale_marker(),
            le: *le,
          })
          .collect(),
        sample_count: prom_stale_marker(),
        sample_sum: prom_stale_marker(),
      }),
      None,
      timestamp,
      now,
      MetricType::Histogram,
      prom_source,
    )]
  }

  pub(super) fn produce_metrics(
    &mut self,
    previous_aggregation: Option<AggregationType>,
    metric_id: &MetricId,
    config: &AggregationConfig,
    timestamp: u64,
    now: Instant,
    prom_source: bool,
    generation: u64,
  ) -> Vec<Option<ParsedMetric>> {
    // If the previous aggregation is not the same type or doesn't have the same buckets there is
    // nothing we can do currently.
    let Some(AggregationType::Histogram(previous_aggregation)) = previous_aggregation else {
      return vec![];
    };

    if previous_aggregation.buckets.len() != self.buckets.len() {
      return vec![];
    }

    for ((_, previous_le), (_, current_le)) in
      previous_aggregation.buckets.iter().zip(self.buckets.iter())
    {
      if *previous_le != *current_le {
        return vec![];
      }
    }

    let Ok(buckets) = previous_aggregation
      .buckets
      .iter()
      .zip(self.buckets.iter_mut())
      .map(|((previous_counter, _), (current_counter, current_le))| {
        let Some((count, _)) =
          current_counter.produce_value(Some(previous_counter), metric_id, config, generation)
        else {
          bail!("cannot produce buckets");
        };
        Ok(HistogramBucket {
          count,
          le: *current_le,
        })
      })
      .try_collect()
    else {
      return vec![];
    };

    let Some((sample_count, _)) = self.sample_count.produce_value(
      Some(&previous_aggregation.sample_count),
      metric_id,
      config,
      generation,
    ) else {
      return vec![];
    };

    let Some((sample_sum, _)) = self.sample_sum.produce_value(
      Some(&previous_aggregation.sample_sum),
      metric_id,
      config,
      generation,
    ) else {
      return vec![];
    };

    let histogram_data = HistogramData {
      buckets,
      sample_count,
      sample_sum,
    };

    vec![make_metric(
      make_name(
        config.histograms.prefix.as_ref().map_or("", |p| p.as_str()),
        metric_id.name(),
        "",
      ),
      metric_id.tags().to_vec(),
      MetricValue::Histogram(histogram_data),
      None,
      timestamp,
      now,
      MetricType::Histogram,
      prom_source,
    )]
  }
}
