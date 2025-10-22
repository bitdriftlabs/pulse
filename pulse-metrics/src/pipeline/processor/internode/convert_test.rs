// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::*;
use crate::protos::metric::{HistogramData, SummaryData};
use crate::test::make_tag;
use bytes::Bytes;
use quickcheck_macros::quickcheck;
use std::time::Instant;

#[test]
fn test_convert() {
  let m = ParsedMetric::new(
    Metric::new(
      MetricId::new(
        Bytes::from_static(b"hello"),
        Some(MetricType::BulkTimer),
        vec![make_tag("yummy", "cheese")],
        false,
      )
      .unwrap(),
      Some(0.2_f64),
      1234,
      MetricValue::BulkTimer(vec![1.5_f64]),
    ),
    MetricSource::Carbon(Bytes::from_static(b"original line")),
    Instant::now(),
    DownstreamId::LocalOrigin,
  );

  let m_proto = ProtoMetric::from(&m);
  let m2 = proto_metric_to_parsed_metric(m_proto).unwrap();
  assert_eq!(m, m2);

  let m = ParsedMetric::new(
    Metric::new(
      MetricId::new(
        Bytes::from_static(b"hello"),
        Some(MetricType::DeltaGauge),
        vec![make_tag("yummy", "cheese")],
        false,
      )
      .unwrap(),
      Some(0.2_f64),
      1234,
      MetricValue::Simple(1.5_f64),
    ),
    MetricSource::Carbon(Bytes::from_static(b"original line")),
    Instant::now(),
    DownstreamId::LocalOrigin,
  );

  let m_proto = ProtoMetric::from(&m);
  let m2 = proto_metric_to_parsed_metric(m_proto).unwrap();
  assert_eq!(m, m2);

  let m = ParsedMetric::new(
    Metric::new(
      MetricId::new(
        Bytes::from_static(b"hello"),
        Some(MetricType::Counter(CounterType::Absolute)),
        vec![],
        false,
      )
      .unwrap(),
      None,
      1234,
      MetricValue::Simple(1.5_f64),
    ),
    MetricSource::Carbon(Bytes::from_static(b"original line")),
    Instant::now(),
    DownstreamId::InflowProvided("hello".into()),
  );

  let m_proto = ProtoMetric::from(&m);
  let m2 = proto_metric_to_parsed_metric(m_proto).unwrap();
  assert_eq!(m, m2);

  let m = ParsedMetric::new(
    Metric::new(
      MetricId::new(
        "histogram".into(),
        Some(MetricType::Histogram),
        vec![make_tag("hello", "world")],
        false,
      )
      .unwrap(),
      None,
      0,
      MetricValue::Histogram(HistogramData {
        buckets: vec![
          HistogramBucket {
            count: 25.0,
            le: 0.5,
          },
          HistogramBucket {
            count: 30.0,
            le: 1.0,
          },
        ],
        sample_count: 50.0,
        sample_sum: 100.0,
      }),
    ),
    MetricSource::PromRemoteWrite,
    Instant::now(),
    DownstreamId::UnixDomainSocket("hello".to_string().into()),
  );

  let m_proto = ProtoMetric::from(&m);
  let m2 = proto_metric_to_parsed_metric(m_proto).unwrap();
  assert_eq!(m, m2);

  let m = ParsedMetric::new(
    Metric::new(
      MetricId::new(
        "summary".into(),
        Some(MetricType::Summary),
        vec![make_tag("hello", "world")],
        false,
      )
      .unwrap(),
      None,
      0,
      MetricValue::Summary(SummaryData {
        quantiles: vec![
          SummaryBucket {
            quantile: 0.5,
            value: 30.0,
          },
          SummaryBucket {
            quantile: 0.9,
            value: 40.0,
          },
        ],
        sample_count: 50.0,
        sample_sum: 100.0,
      }),
    ),
    MetricSource::PromRemoteWrite,
    Instant::now(),
    DownstreamId::IpAddress("127.0.0.1".parse().unwrap()),
  );

  let m_proto = ProtoMetric::from(&m);
  let m2 = proto_metric_to_parsed_metric(m_proto).unwrap();
  assert_eq!(m, m2);
}

#[quickcheck]
fn parsed_metric_roundtrip_metric(metric: Metric) -> anyhow::Result<()> {
  let m = ParsedMetric::new(
    metric,
    MetricSource::PromRemoteWrite,
    Instant::now(),
    DownstreamId::IpAddress("::1".parse().unwrap()),
  );
  let m_proto = ProtoMetric::from(&m);
  let m2 = proto_metric_to_parsed_metric(m_proto)?;
  if m.eq(&m2) {
    Ok(())
  } else {
    Err(anyhow::anyhow!("m = {m:?}, m2 = {m2:?}"))
  }
}
