// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::test::integration::{
  FakeHttpUpstream,
  FakeWireUpstream,
  Helper,
  HelperBindResolver,
  PromClient,
};
use http::{HeaderMap, StatusCode};
use itertools::Itertools;
use pretty_assertions::assert_eq;
use prom_remote_write::prom_remote_write_server_config::ParseConfig;
use prometheus::labels;
use pulse_metrics::protos::metric::ParsedMetric;
use pulse_metrics::test::{make_metric, parse_carbon_metrics};
use pulse_protobuf::protos::pulse::config::inflow::v1::prom_remote_write;
use reusable_fmt::{fmt, fmt_reuse};

fmt_reuse! {
INFLOW = r#"
pipeline:
  inflows:
    prom_remote_write:
      routes: ["processor:buffer"]
      prom_remote_write:
        bind: "inflow:prom"

  processors:
    buffer:
      routes: ["processor:populate_cache"]
      buffer:
        max_buffered_metrics: 100000
        num_consumers: 1

    populate_cache:
      routes: ["processor:elision"]
      populate_cache: {{}}

    elision:
      routes: ["outflow:tcp"]
      elision:
        emit: {{ ratio: 0.1 }}

  outflows:
    tcp:
      tcp:
        common:
          send_to: "{fake_upstream}"
          protocol:
            carbon: {{}}
"#;
}

#[tokio::test]
async fn remote_write() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:prom"], &[]).await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let helper = Helper::new(
    &fmt!(
      INFLOW,
      fake_upstream = bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
  )
  .await;
  let client = PromClient::new(bind_resolver.local_tcp_addr("inflow:prom")).await;

  let current_time = 1;
  for i in 0 .. 10 {
    let metrics = (0 .. 5)
      .map(|j| {
        make_metric(
          "node_request.count",
          &[("tag", "value-0"), ("source", "server-0")],
          current_time + (i * 5) + j,
        )
      })
      .collect_vec();
    client.send(metrics).await;
  }

  assert_eq!(
    parse_carbon_metrics(&[
      &format!(
        "\"node_request.count\" 0 {current_time} \"source\"=\"server-0\" \"tag\"=\"value-0\"\n"
      ),
      &format!(
        "\"node_request.count\" 0 {} \"source\"=\"server-0\" \"tag\"=\"value-0\"\n",
        current_time + 9
      ),
      &format!(
        "\"node_request.count\" 0 {} \"source\"=\"server-0\" \"tag\"=\"value-0\"\n",
        current_time + 19
      ),
      &format!(
        "\"node_request.count\" 0 {} \"source\"=\"server-0\" \"tag\"=\"value-0\"\n",
        current_time + 29
      ),
      &format!(
        "\"node_request.count\" 0 {} \"source\"=\"server-0\" \"tag\"=\"value-0\"\n",
        current_time + 39
      ),
      &format!(
        "\"node_request.count\" 0 {} \"source\"=\"server-0\" \"tag\"=\"value-0\"\n",
        current_time + 49
      ),
    ]),
    upstream.wait_for_metrics().await
  );

  helper.shutdown().await;
}

fmt_reuse! {
MAX_BATCH_SIZE = r#"
  pipeline:
    inflows:
      prom_remote_write:
        routes: ["outflow:prom"]
        prom_remote_write:
          bind: "inflow:prom"

    outflows:
      prom:
        prom_remote_write:
          send_to: "http://{fake_upstream}/api/v1/prom/write"
          batch_max_size: 1024
  "#;
}

#[tokio::test]
async fn max_batch_size() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:prom"], &[]).await;
  let mut upstream = FakeHttpUpstream::new_prom(
    "fake_upstream",
    bind_resolver.clone(),
    ParseConfig::default(),
  )
  .await;
  let helper = Helper::new(
    &fmt!(
      MAX_BATCH_SIZE,
      fake_upstream = bind_resolver.local_tcp_addr("fake_upstream"),
    ),
    bind_resolver.clone(),
  )
  .await;
  let client = PromClient::new(bind_resolver.local_tcp_addr("inflow:prom")).await;

  let metrics = (0 .. 150)
    .map(|i| make_metric(&format!("metric_{i:03}"), &[], 1))
    .collect_vec();
  client.send(metrics).await;
  assert_eq!(upstream.wait_for_metrics().await.1.len(), 75);
  assert_eq!(upstream.wait_for_metrics().await.1.len(), 75);

  helper.shutdown().await;
}

fmt_reuse! {
LYFT_SPECIFIC_CONFIG = r#"
  pipeline:
    inflows:
      prom_remote_write:
        routes: ["outflow:prom1", "outflow:prom2"]
        prom_remote_write:
          bind: "inflow:prom"

    outflows:
      prom1:
        prom_remote_write:
          send_to: "http://{fake_upstream1}/api/v1/prom/write"
          request_headers:
            - name: extra
              value:
                inline: header
          lyft_specific_config:
            general_storage_policy: foo
            instance_metrics_storage_policy: bar
            cloudwatch_metrics_storage_policy: baz
          retry_policy:
            max_retries: 0
            offload_queue:
              loopback_for_test: {{}}
      prom2:
        prom_remote_write:
          send_to: "http://{fake_upstream2}/api/v1/prom/write"
          lyft_specific_config:
            general_storage_policy: blah
          retry_policy:
            max_retries: 0
            offload_queue:
              loopback_for_test: {{}}
  "#;
}

async fn expect_metric(
  fake_upstream: &mut FakeHttpUpstream,
  storage_policy: &str,
  metric: ParsedMetric,
) -> HeaderMap {
  let (headers, metrics) = fake_upstream.wait_for_metrics().await;
  assert_eq!(headers.get("M3-Metrics-Type").unwrap(), "aggregated");
  assert_eq!(headers.get("M3-Storage-Policy").unwrap(), storage_policy);
  assert_eq!(metrics, vec![metric]);
  headers
}

#[tokio::test]
async fn lyft_remote_write() {
  let bind_resolver =
    HelperBindResolver::new(&["fake_upstream1", "fake_upstream2", "inflow:prom"], &[]).await;
  let mut upstream1 = FakeHttpUpstream::new_prom(
    "fake_upstream1",
    bind_resolver.clone(),
    ParseConfig::default(),
  )
  .await;
  let mut upstream2 = FakeHttpUpstream::new_prom(
    "fake_upstream2",
    bind_resolver.clone(),
    ParseConfig::default(),
  )
  .await;
  let helper = Helper::new(
    &fmt!(
      LYFT_SPECIFIC_CONFIG,
      fake_upstream1 = bind_resolver.local_tcp_addr("fake_upstream1"),
      fake_upstream2 = bind_resolver.local_tcp_addr("fake_upstream2")
    ),
    bind_resolver.clone(),
  )
  .await;
  let client = PromClient::new(bind_resolver.local_tcp_addr("inflow:prom")).await;

  client.send(vec![make_metric("foo", &[], 1)]).await;
  let headers = expect_metric(&mut upstream1, "foo", make_metric("foo", &[], 1)).await;
  assert_eq!(headers.get("extra").unwrap(), "header");
  expect_metric(&mut upstream2, "blah", make_metric("foo", &[], 1)).await;

  client
    .send(vec![make_metric("foo:host", &[("host", "foo")], 1)])
    .await;
  upstream1.add_failure_response_code(StatusCode::INTERNAL_SERVER_ERROR);
  expect_metric(
    &mut upstream1,
    "bar",
    make_metric("foo:host", &[("host", "foo")], 1),
  )
  .await;
  expect_metric(
    &mut upstream2,
    "blah",
    make_metric("foo:host", &[("host", "foo")], 1),
  )
  .await;

  client
    .send(vec![make_metric(
      "foo:infra:aws:blah",
      &[("source", "non_statsd")],
      1,
    )])
    .await;
  expect_metric(
    &mut upstream1,
    "baz",
    make_metric("foo:infra:aws:blah", &[("source", "non_statsd")], 1),
  )
  .await;
  expect_metric(
    &mut upstream2,
    "blah",
    make_metric("foo:infra:aws:blah", &[("source", "non_statsd")], 1),
  )
  .await;

  upstream1.add_failure_response_code(StatusCode::BAD_REQUEST);
  upstream2.add_failure_response_code(StatusCode::BAD_REQUEST);
  client
    .send(vec![make_metric(
      "foo:infra:aws:blah",
      &[("source", "non_statsd")],
      1,
    )])
    .await;

  helper
    .stats_helper()
    .wait_for_counter_eq(
      1,
      "pulse_proxy:pipeline:outflow:prom1:offload_queue_tx",
      &labels! {},
    )
    .await;
  helper
    .stats_helper()
    .wait_for_counter_eq(
      1,
      "pulse_proxy:pipeline:messages_outgoing",
      &labels! {"src" => "prom1", "src_type" => "outflow", "status" => "failed"},
    )
    .await;

  helper.shutdown().await;
}
