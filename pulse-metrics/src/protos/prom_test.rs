// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::protos::metric::{
  DownstreamId,
  HistogramData,
  Metric,
  MetricId,
  MetricSource,
  MetricType,
  MetricValue,
  ParseError,
  ParsedMetric,
  SummaryData,
};
use crate::protos::prom::{
  ChangedTypeTracker,
  HistogramBucket,
  MetadataType,
  SummaryBucket,
  ToWriteRequestOptions,
  from_write_request,
  make_label,
  make_timeseries,
  to_write_request,
};
use crate::test::{make_gauge, make_metric_ex};
use assert_matches::assert_matches;
use bd_proto::protos::prometheus::prompb;
use config::inflow::v1::prom_remote_write::prom_remote_write_server_config::ParseConfig;
use itertools::Itertools;
use pretty_assertions::assert_eq;
use prompb::remote::WriteRequest;
use prompb::types::MetricMetadata;
use prompb::types::metric_metadata::MetricType as PromMetricType;
use pulse_protobuf::protos::pulse::config;
use std::time::Instant;

#[test]
fn invalid_histogram_or_summary() {
  let write_request = WriteRequest {
    timeseries: vec![make_timeseries("foo".into(), vec![], 23.0, 1000, vec![])],
    metadata: vec![MetricMetadata {
      type_: PromMetricType::SUMMARY.into(),
      metric_family_name: "foo".into(),
      ..Default::default()
    }],
    ..Default::default()
  };
  let (metrics, errors) = from_write_request(write_request, &ParseConfig::default());
  assert!(metrics.is_empty());
  assert_eq!(1, errors.len());
  assert_matches!(
    &errors[0],
    ParseError::PromRemoteWrite(message)
      if message == "foo: invalid histogram or summary timeseries"
  );
}

#[test]
fn name_conversion() {
  let metric = make_metric_ex(
    "foo.bar",
    &[("tag1", "value1"), ("tag2.with.dots", "value2")],
    1,
    Some(MetricType::Gauge),
    None,
    MetricValue::Simple(1.0),
    MetricSource::Aggregation { prom_source: false },
    DownstreamId::LocalOrigin,
    None,
  );
  let write_request = to_write_request(
    vec![metric],
    &ToWriteRequestOptions {
      metadata: MetadataType::Normal,
      convert_name: true,
    },
    &ChangedTypeTracker::new_for_test(),
  );
  let (metrics, errors) = from_write_request(write_request, &ParseConfig::default());
  assert!(errors.is_empty());
  assert_eq!(
    metrics,
    vec![
      make_gauge(
        "foo:bar",
        &[("tag1", "value1"), ("tag2_with_dots", "value2")],
        1,
        1.0
      )
      .into_metric()
      .0
    ]
  );
}

#[test]
fn summary_round_trip() {
  let write_request = WriteRequest {
    timeseries: vec![
      make_timeseries(
        "foo".into(),
        vec![],
        23.0,
        1000,
        vec![make_label("quantile".into(), "0.5".into())],
      ),
      make_timeseries(
        "foo".into(),
        vec![],
        25.0,
        1000,
        vec![make_label("quantile".into(), "0.9".into())],
      ),
      make_timeseries(
        "foo".into(),
        vec![],
        30.0,
        1000,
        vec![make_label("quantile".into(), "0.99".into())],
      ),
      make_timeseries("foo_count".into(), vec![], 30.0, 1000, vec![]),
      make_timeseries("foo_sum".into(), vec![], 10.0, 1000, vec![]),
    ],
    metadata: vec![MetricMetadata {
      type_: PromMetricType::SUMMARY.into(),
      metric_family_name: "foo".into(),
      ..Default::default()
    }],
    ..Default::default()
  };

  let (metrics, errors) = from_write_request(write_request, &ParseConfig::default());
  assert!(errors.is_empty());
  assert_eq!(
    metrics,
    vec![Metric::new(
      MetricId::new("foo".into(), Some(MetricType::Summary), vec![], false).unwrap(),
      None,
      1,
      MetricValue::Summary(SummaryData {
        quantiles: vec![
          SummaryBucket {
            quantile: 0.5,
            value: 23.0
          },
          SummaryBucket {
            quantile: 0.9,
            value: 25.0
          },
          SummaryBucket {
            quantile: 0.99,
            value: 30.0
          }
        ],
        sample_count: 30.0,
        sample_sum: 10.0
      })
    )]
  );

  let write_request = to_write_request(
    metrics
      .into_iter()
      .map(|metric| {
        ParsedMetric::new(
          metric,
          MetricSource::PromRemoteWrite,
          Instant::now(),
          DownstreamId::LocalOrigin,
        )
      })
      .collect_vec(),
    &ToWriteRequestOptions {
      metadata: MetadataType::Normal,
      convert_name: true,
    },
    &ChangedTypeTracker::new_for_test(),
  );

  let (metrics, errors) = from_write_request(write_request, &ParseConfig::default());
  assert!(errors.is_empty());
  assert_eq!(
    metrics,
    vec![Metric::new(
      MetricId::new("foo".into(), Some(MetricType::Summary), vec![], false).unwrap(),
      None,
      1,
      MetricValue::Summary(SummaryData {
        quantiles: vec![
          SummaryBucket {
            quantile: 0.5,
            value: 23.0
          },
          SummaryBucket {
            quantile: 0.9,
            value: 25.0
          },
          SummaryBucket {
            quantile: 0.99,
            value: 30.0
          }
        ],
        sample_count: 30.0,
        sample_sum: 10.0
      })
    )]
  );
}

#[test]
fn histogram_round_trip() {
  let write_request = WriteRequest {
    timeseries: vec![
      make_timeseries(
        "foo_bucket".into(),
        vec![],
        23.0,
        1000,
        vec![make_label("le".into(), "0.05".into())],
      ),
      make_timeseries(
        "foo_bucket".into(),
        vec![],
        25.0,
        1000,
        vec![make_label("le".into(), "0.1".into())],
      ),
      make_timeseries(
        "foo_bucket".into(),
        vec![],
        30.0,
        1000,
        vec![make_label("le".into(), "+Inf".into())],
      ),
      make_timeseries("foo_count".into(), vec![], 30.0, 1000, vec![]),
      make_timeseries("foo_sum".into(), vec![], 10.0, 1000, vec![]),
    ],
    metadata: vec![MetricMetadata {
      type_: PromMetricType::HISTOGRAM.into(),
      metric_family_name: "foo".into(),
      ..Default::default()
    }],
    ..Default::default()
  };

  let (metrics, errors) = from_write_request(write_request, &ParseConfig::default());
  assert!(errors.is_empty());
  assert_eq!(
    metrics,
    vec![Metric::new(
      MetricId::new("foo".into(), Some(MetricType::Histogram), vec![], false).unwrap(),
      None,
      1,
      MetricValue::Histogram(HistogramData {
        buckets: vec![
          HistogramBucket {
            count: 23.0,
            le: 0.05
          },
          HistogramBucket {
            count: 25.0,
            le: 0.1
          }
        ],
        sample_count: 30.0,
        sample_sum: 10.0
      })
    )]
  );

  let write_request = to_write_request(
    metrics
      .into_iter()
      .map(|metric| {
        ParsedMetric::new(
          metric,
          MetricSource::PromRemoteWrite,
          Instant::now(),
          DownstreamId::LocalOrigin,
        )
      })
      .collect_vec(),
    &ToWriteRequestOptions {
      metadata: MetadataType::Normal,
      convert_name: true,
    },
    &ChangedTypeTracker::new_for_test(),
  );

  let (metrics, errors) = from_write_request(write_request, &ParseConfig::default());
  assert!(errors.is_empty());
  assert_eq!(
    metrics,
    vec![Metric::new(
      MetricId::new("foo".into(), Some(MetricType::Histogram), vec![], false).unwrap(),
      None,
      1,
      MetricValue::Histogram(HistogramData {
        buckets: vec![
          HistogramBucket {
            count: 23.0,
            le: 0.05
          },
          HistogramBucket {
            count: 25.0,
            le: 0.1
          }
        ],
        sample_count: 30.0,
        sample_sum: 10.0
      })
    )]
  );
}

#[test]
fn flatten_histogram_and_summary() {
  let write_request = WriteRequest {
    timeseries: vec![
      make_timeseries(
        "foo_bucket".into(),
        vec![],
        23.0,
        1000,
        vec![make_label("le".into(), "0.05".into())],
      ),
      make_timeseries(
        "foo_bucket".into(),
        vec![],
        25.0,
        1000,
        vec![make_label("le".into(), "0.1".into())],
      ),
      make_timeseries(
        "foo_bucket".into(),
        vec![],
        30.0,
        1000,
        vec![make_label("le".into(), "+Inf".into())],
      ),
      make_timeseries("foo_count".into(), vec![], 30.0, 1000, vec![]),
      make_timeseries("foo_sum".into(), vec![], 10.0, 1000, vec![]),
      make_timeseries(
        "bar".into(),
        vec![],
        23.0,
        1000,
        vec![make_label("quantile".into(), "0.5".into())],
      ),
      make_timeseries(
        "bar".into(),
        vec![],
        25.0,
        1000,
        vec![make_label("quantile".into(), "0.9".into())],
      ),
      make_timeseries(
        "bar".into(),
        vec![],
        30.0,
        1000,
        vec![make_label("quantile".into(), "0.99".into())],
      ),
      make_timeseries("bar_count".into(), vec![], 30.0, 1000, vec![]),
      make_timeseries("bar_sum".into(), vec![], 10.0, 1000, vec![]),
    ],
    metadata: vec![
      MetricMetadata {
        type_: PromMetricType::HISTOGRAM.into(),
        metric_family_name: "foo".into(),
        ..Default::default()
      },
      MetricMetadata {
        type_: PromMetricType::SUMMARY.into(),
        metric_family_name: "bar".into(),
        ..Default::default()
      },
    ],
    ..Default::default()
  };

  let (metrics, errors) = from_write_request(
    write_request,
    &ParseConfig {
      flatten_histogram_and_summary: true,
      ..Default::default()
    },
  );
  assert!(errors.is_empty());
  assert_eq!(metrics.len(), 10);

  // Verify all metrics are simple and have no type.
  for metric in metrics {
    assert_eq!(metric.get_id().mtype(), None);
    assert_matches!(metric.value, MetricValue::Simple(_));
  }
}
