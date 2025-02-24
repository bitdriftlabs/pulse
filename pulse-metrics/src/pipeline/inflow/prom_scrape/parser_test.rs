// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::pipeline::inflow::prom_scrape::parser::parse_as_metrics;
use crate::protos::metric::{
  DownstreamId,
  HistogramBucket,
  HistogramData,
  MetricSource,
  MetricType,
  MetricValue,
  SummaryBucket,
  SummaryData,
  default_timestamp,
};
use crate::test::{make_abs_counter_with_metadata, make_gauge_with_metadata, make_metric_ex};
use pretty_assertions::assert_eq;
use pulse_common::metadata::Metadata;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;
use time::OffsetDateTime;
use time::macros::datetime;

const TIMESTAMP: OffsetDateTime = datetime!(2021-02-04 04:05:06 UTC);

fn expected_metadata() -> Metadata {
  Metadata::new(
    "",
    "",
    "",
    &BTreeMap::default(),
    &BTreeMap::default(),
    None,
    "",
    "",
    None,
  )
}

fn test_metadata() -> Arc<Metadata> {
  Arc::new(expected_metadata())
}

#[test]
fn counter() {
  let exp = r"
            # HELP uptime A counter
            # TYPE uptime counter
            uptime 123.0 1612411506789
            ";

  assert_eq!(
    parse_as_metrics(
      exp,
      default_timestamp(),
      Instant::now(),
      Some(&test_metadata())
    )
    .unwrap(),
    vec![make_abs_counter_with_metadata(
      "uptime",
      &[],
      TIMESTAMP.unix_timestamp().try_into().unwrap(),
      123.0,
      expected_metadata()
    )]
  );
}

#[test]
fn mixed() {
  let exp = r"
            # TYPE uptime counter
            uptime 123.0 1612411506789
            # TYPE temperature gauge
            temperature -1.5 1612411506789
            # TYPE launch_count counter
            launch_count 10.0
            ";

  let timestamp = default_timestamp();
  assert_eq!(
    parse_as_metrics(exp, timestamp, Instant::now(), Some(&test_metadata())).unwrap(),
    vec![
      make_abs_counter_with_metadata(
        "uptime",
        &[],
        TIMESTAMP.unix_timestamp().try_into().unwrap(),
        123.0,
        expected_metadata()
      ),
      make_gauge_with_metadata(
        "temperature",
        &[],
        TIMESTAMP.unix_timestamp().try_into().unwrap(),
        -1.5,
        expected_metadata()
      ),
      make_abs_counter_with_metadata("launch_count", &[], timestamp, 10.0, expected_metadata())
    ]
  );
}

#[test]
fn test_histogram() {
  let exp = r#"
            # A histogram, which has a pretty complex representation in the text format:
            # HELP http_request_duration_seconds A histogram of the request duration.
            # TYPE http_request_duration_seconds histogram
            http_request_duration_seconds_bucket{le="0.05"} 24054 1612411506789
            http_request_duration_seconds_bucket{le="0.1"} 33444 1612411506789
            http_request_duration_seconds_bucket{le="0.2"} 100392 1612411506789
            http_request_duration_seconds_bucket{le="0.5"} 129389 1612411506789
            http_request_duration_seconds_bucket{le="1"} 133988 1612411506789
            http_request_duration_seconds_bucket{le="+Inf"} 144320 1612411506789
            http_request_duration_seconds_sum 53423 1612411506789
            http_request_duration_seconds_count 144320 1612411506789
            "#;

  assert_eq!(
    parse_as_metrics(
      exp,
      default_timestamp(),
      Instant::now(),
      Some(&test_metadata())
    )
    .unwrap(),
    vec![make_metric_ex(
      "http_request_duration_seconds",
      &[],
      TIMESTAMP.unix_timestamp().try_into().unwrap(),
      Some(MetricType::Histogram),
      None,
      MetricValue::Histogram(HistogramData {
        buckets: vec![
          HistogramBucket {
            le: 0.05,
            count: 24054.0
          },
          HistogramBucket {
            le: 0.1,
            count: 33444.0
          },
          HistogramBucket {
            le: 0.2,
            count: 100_392.0
          },
          HistogramBucket {
            le: 0.5,
            count: 129_389.0
          },
          HistogramBucket {
            le: 1.0,
            count: 133_988.0
          }
        ],
        sample_sum: 53423.0,
        sample_count: 144_320.0
      }),
      MetricSource::PromRemoteWrite,
      DownstreamId::LocalOrigin,
      Some(expected_metadata())
    )]
  );
}

#[test]
fn test_summary() {
  let exp = r#"
            # HELP rpc_duration_seconds A summary of the RPC duration in seconds.
            # TYPE rpc_duration_seconds summary
            rpc_duration_seconds{service="a",quantile="0.01"} 3102 1612411506789
            rpc_duration_seconds{service="a",quantile="0.05"} 3272 1612411506789
            rpc_duration_seconds{service="a",quantile="0.5"} 4773 1612411506789
            rpc_duration_seconds{service="a",quantile="0.9"} 9001 1612411506789
            rpc_duration_seconds{service="a",quantile="0.99"} 76656 1612411506789
            rpc_duration_seconds_sum{service="a"} 1.7560473e+07 1612411506789
            rpc_duration_seconds_count{service="a"} 2693 1612411506789
            "#;

  assert_eq!(
    parse_as_metrics(
      exp,
      default_timestamp(),
      Instant::now(),
      Some(&test_metadata())
    )
    .unwrap(),
    vec![make_metric_ex(
      "rpc_duration_seconds",
      &[("service", "a")],
      TIMESTAMP.unix_timestamp().try_into().unwrap(),
      Some(MetricType::Summary),
      None,
      MetricValue::Summary(SummaryData {
        quantiles: vec![
          SummaryBucket {
            quantile: 0.01,
            value: 3102.0
          },
          SummaryBucket {
            quantile: 0.05,
            value: 3272.0
          },
          SummaryBucket {
            quantile: 0.5,
            value: 4773.0
          },
          SummaryBucket {
            quantile: 0.9,
            value: 9001.0
          },
          SummaryBucket {
            quantile: 0.99,
            value: 76656.0
          }
        ],
        sample_sum: 1.756_047_3e+07,
        sample_count: 2693.0
      }),
      MetricSource::PromRemoteWrite,
      DownstreamId::LocalOrigin,
      Some(expected_metadata())
    )]
  );
}

#[test]
fn test_invalid() {
  assert!(
    parse_as_metrics(
      "not prom",
      default_timestamp(),
      Instant::now(),
      Some(&test_metadata())
    )
    .is_err()
  );
}
