// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./parser_test.rs"]
mod parser_test;

use crate::protos::metric::{
  CounterType,
  DownstreamId,
  HistogramBucket,
  HistogramData,
  Metric,
  MetricId,
  MetricSource,
  MetricType,
  MetricValue,
  ParsedMetric,
  SummaryBucket,
  SummaryData,
  TagValue,
};
use prometheus_parser::{MetricGroup, ParserError};
use pulse_common::LossyIntoToFloat;
use pulse_common::metadata::Metadata;
use std::sync::Arc;
use std::time::Instant;

pub fn parse_as_metrics(
  data: &str,
  timestamp: u64,
  now: Instant,
  metadata: Option<&Arc<Metadata>>,
  extra_tags: &[TagValue],
) -> Result<Vec<ParsedMetric>, ParserError> {
  let parsed = prometheus_parser::parse_text(data)?;

  Ok(
    parsed
      .into_iter()
      .flat_map(|group| sample_as_metric(group, timestamp, now, metadata, extra_tags))
      .collect(),
  )
}

#[allow(clippy::cast_possible_wrap)]
fn sample_as_metric(
  group: MetricGroup,
  timestamp: u64,
  now: Instant,
  metadata: Option<&Arc<Metadata>>,
  extra_tags: &[TagValue],
) -> Vec<ParsedMetric> {
  let mut events = Vec::new();
  match group.metrics {
    prometheus_parser::GroupKind::Summary(summaries) => {
      for (key, summary) in summaries {
        events.push(make_metric(
          &group.name,
          &key,
          Some(MetricType::Summary),
          MetricValue::Summary(SummaryData {
            quantiles: summary
              .quantiles
              .iter()
              .map(|quantile| SummaryBucket {
                quantile: quantile.quantile,
                value: quantile.value,
              })
              .collect(),
            sample_count: summary.count.lossy_to_f64(),
            sample_sum: summary.sum,
          }),
          timestamp,
          now,
          metadata.cloned(),
          extra_tags,
        ));
      }
    },
    prometheus_parser::GroupKind::Histogram(histograms) => {
      for (key, histogram) in histograms {
        events.push(make_metric(
          &group.name,
          &key,
          Some(MetricType::Histogram),
          MetricValue::Histogram(HistogramData {
            // TODO(mattklein123): We should have similar error checking here like we do on the
            // standard remote write ingestion path. We should unify that logic.
            buckets: histogram
              .buckets
              .iter()
              .filter_map(|bucket| {
                if bucket.bucket.is_infinite() {
                  None
                } else {
                  Some(HistogramBucket {
                    le: bucket.bucket,
                    count: bucket.count.lossy_to_f64(),
                  })
                }
              })
              .collect(),
            sample_count: histogram.count.lossy_to_f64(),
            sample_sum: histogram.sum,
          }),
          timestamp,
          now,
          metadata.cloned(),
          extra_tags,
        ));
      }
    },
    prometheus_parser::GroupKind::Gauge(gauges) => {
      for (key, gauge) in gauges {
        events.push(make_metric(
          &group.name,
          &key,
          Some(MetricType::Gauge),
          MetricValue::Simple(gauge.value),
          timestamp,
          now,
          metadata.cloned(),
          extra_tags,
        ));
      }
    },
    prometheus_parser::GroupKind::Counter(counters) => {
      for (key, counter) in counters {
        events.push(make_metric(
          &group.name,
          &key,
          Some(MetricType::Counter(CounterType::Absolute)),
          MetricValue::Simple(counter.value),
          timestamp,
          now,
          metadata.cloned(),
          extra_tags,
        ));
      }
    },
    prometheus_parser::GroupKind::Untyped(untypeds) => {
      // TODO(mattklein123): The old code converted untyped metrics to gauges. Is this the right
      // thing to do?
      for (key, untyped) in untypeds {
        events.push(make_metric(
          &group.name,
          &key,
          Some(MetricType::Gauge),
          MetricValue::Simple(untyped.value),
          timestamp,
          now,
          metadata.cloned(),
          extra_tags,
        ));
      }
    },
  }

  events
}

fn make_metric(
  name: &str,
  group_key: &prometheus_parser::GroupKey,
  mtype: Option<MetricType>,
  value: MetricValue,
  now_unix: u64,
  now: Instant,
  metadata: Option<Arc<Metadata>>,
  extra_tags: &[TagValue],
) -> ParsedMetric {
  let timestamp = group_key.timestamp.map_or(now_unix, |timestamp| {
    (timestamp / 1000).try_into().unwrap_or(now_unix)
  });

  let mut metric = ParsedMetric::new(
    // TODO(mattklein123): Is LocalOrigin the right ID for this?
    Metric::new(
      // TODO(mattklein123): Handle failure/unwrap below.
      MetricId::new(
        name.as_bytes().to_vec().into(),
        mtype,
        group_key
          .labels
          .iter()
          .map(|(k, v)| TagValue {
            tag: k.as_bytes().to_vec().into(),
            value: v.as_bytes().to_vec().into(),
          })
          .chain(extra_tags.iter().cloned())
          .collect(),
        false,
      )
      .unwrap(),
      None,
      timestamp,
      value,
    ),
    MetricSource::PromRemoteWrite,
    now,
    DownstreamId::LocalOrigin,
  );
  metric.set_metadata(metadata);
  metric
}
