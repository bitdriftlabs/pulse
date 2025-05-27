// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::test::integration::{FakeHttpUpstream, Helper, HelperBindResolver, OtlpClient};
use pretty_assertions::assert_eq;
use protobuf::Message;
use pulse_metrics::clients::http::HttpRemoteWriteClient;
use pulse_metrics::protos::metric::{
  DownstreamId,
  HistogramBucket,
  HistogramData,
  MetricSource,
  MetricType,
  MetricValue,
  SummaryBucket,
  SummaryData,
};
use pulse_metrics::test::{make_abs_counter, make_gauge, make_metric, make_metric_ex};
use pulse_protobuf::protos::opentelemetry::common::any_value::Value;
use pulse_protobuf::protos::opentelemetry::common::{AnyValue, InstrumentationScope, KeyValue};
use pulse_protobuf::protos::opentelemetry::metrics::metric::Data;
use pulse_protobuf::protos::opentelemetry::metrics::{
  AggregationTemporality,
  Metric,
  NumberDataPoint,
  ResourceMetrics,
  ScopeMetrics,
  Sum,
  number_data_point,
};
use pulse_protobuf::protos::opentelemetry::metrics_service::ExportMetricsServiceRequest;
use pulse_protobuf::protos::opentelemetry::resource::Resource;
use reusable_fmt::{fmt, fmt_reuse};

fmt_reuse! {
OTLP_INFLOW = r#"
  pipeline:
    inflows:
      otlp:
        routes: ["outflow:otlp"]
        otlp:
          bind: "inflow:otlp"

    outflows:
      otlp:
        otlp:
          send_to: "http://{fake_upstream}/v1/metrics"
          convert_names_to_prometheus: true
  "#;
}

fn make_kv(key: &str, value: &str) -> KeyValue {
  KeyValue {
    key: key.to_string().into(),
    value: Some(AnyValue {
      value: Some(Value::StringValue(value.to_string().into())),
      ..Default::default()
    })
    .into(),
    ..Default::default()
  }
}

#[tokio::test]
async fn otlp() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:otlp"], &[]).await;
  let mut upstream = FakeHttpUpstream::new_otlp("fake_upstream", bind_resolver.clone()).await;
  let helper = Helper::new(
    &fmt!(
      OTLP_INFLOW,
      fake_upstream = bind_resolver.local_tcp_addr("fake_upstream"),
    ),
    bind_resolver.clone(),
  )
  .await;
  let client = OtlpClient::new(bind_resolver.local_tcp_addr("inflow:otlp")).await;

  client
    .send(vec![
      make_metric("foo", &[], 1),
      make_gauge("bar", &[("hello", "world")], 2, 1.3),
      make_abs_counter("baz.something", &[("hello1.blah", "world1")], 3, 4.5),
    ])
    .await;
  assert_eq!(
    vec![
      make_gauge("bar", &[("hello", "world")], 2, 1.3),
      make_abs_counter("baz:something", &[("hello1_blah", "world1")], 3, 4.5),
      make_gauge("foo", &[], 1, 0.0),
    ],
    upstream.wait_for_metrics().await.1
  );

  let histogram = make_metric_ex(
    "foo",
    &[],
    1,
    Some(MetricType::Histogram),
    None,
    MetricValue::Histogram(HistogramData {
      buckets: vec![
        HistogramBucket {
          le: 10.0,
          count: 100.0,
        },
        HistogramBucket {
          le: 20.0,
          count: 200.0,
        },
      ],
      sample_count: 300.0,
      sample_sum: 400.0,
    }),
    MetricSource::Otlp,
    DownstreamId::LocalOrigin,
    None,
  );
  client.send(vec![histogram.clone()]).await;
  assert_eq!(vec![histogram], upstream.wait_for_metrics().await.1);

  let summary = make_metric_ex(
    "foo",
    &[],
    1,
    Some(MetricType::Summary),
    None,
    MetricValue::Summary(SummaryData {
      quantiles: vec![
        SummaryBucket {
          quantile: 0.5,
          value: 1.0,
        },
        SummaryBucket {
          quantile: 0.9,
          value: 10.0,
        },
      ],
      sample_count: 300.0,
      sample_sum: 400.0,
    }),
    MetricSource::Otlp,
    DownstreamId::LocalOrigin,
    None,
  );
  client.send(vec![summary.clone()]).await;
  assert_eq!(vec![summary], upstream.wait_for_metrics().await.1);

  // Test resource/scope merging.
  client
    .client
    .send_write_request(
      ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
          resource: Some(Resource {
            attributes: vec![make_kv("hello", "world"), make_kv("resource", "foo")],
            ..Default::default()
          })
          .into(),
          scope_metrics: vec![ScopeMetrics {
            scope: Some(InstrumentationScope {
              attributes: vec![make_kv("hello", "world2"), make_kv("scope", "bar")],
              ..Default::default()
            })
            .into(),
            metrics: vec![Metric {
              name: "baz".into(),
              data: Some(Data::Sum(Sum {
                data_points: vec![NumberDataPoint {
                  attributes: vec![make_kv("hello", "world3"), make_kv("data", "baz")],
                  time_unix_nano: 3_000_000_000,
                  value: Some(number_data_point::Value::AsDouble(4.5)),
                  ..Default::default()
                }],
                aggregation_temporality: AggregationTemporality::AGGREGATION_TEMPORALITY_CUMULATIVE
                  .into(),
                ..Default::default()
              })),
              ..Default::default()
            }],
            ..Default::default()
          }],
          ..Default::default()
        }],
        ..Default::default()
      }
      .write_to_bytes()
      .unwrap()
      .into(),
      None,
    )
    .await
    .unwrap();
  assert_eq!(
    vec![make_abs_counter(
      "baz",
      &[
        ("data", "baz"),
        ("hello", "world3"),
        ("resource", "foo"),
        ("scope", "bar")
      ],
      3,
      4.5
    ),],
    upstream.wait_for_metrics().await.1
  );

  helper.shutdown().await;
}
