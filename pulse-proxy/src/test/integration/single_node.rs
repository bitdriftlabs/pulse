// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::test::integration::{
  current_time,
  write_all,
  FakeWireUpstream,
  Helper,
  HelperBindResolver,
};
use itertools::Itertools;
use pretty_assertions::assert_eq;
use prometheus::labels;
use pulse_metrics::test::{clean_timestamps, parse_carbon_metrics};
use reusable_fmt::{fmt, fmt_reuse};
use tokio::net::TcpStream;

fmt_reuse! {
DEFAULT_CONFIG = r#"
pipeline:
  inflows:
    tcp:
      routes: ["processor:populate_cache"]
      tcp:
        bind: "inflow:tcp"
        protocol:
          carbon: {{}}

  processors:
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
          send_to: "{}"
          protocol:
            carbon: {{}}
"#;
}

#[tokio::test]
async fn default_config_basic() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:tcp"], &[]).await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let helper = Helper::new(
    &fmt!(
      DEFAULT_CONFIG,
      bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
  )
  .await;
  let mut stream = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp"))
    .await
    .unwrap();

  let lines = vec!["request.count 1001 source=server-0\n"];
  write_all(&mut stream, &lines).await;

  assert_eq!(
    clean_timestamps(parse_carbon_metrics(&lines)),
    clean_timestamps(upstream.wait_for_metrics().await)
  );

  helper
    .stats_helper()
    .wait_for_counter_eq(
      1,
      "pulse_proxy:pipeline:messages_outgoing",
      &labels! {"src" => "tcp", "src_type" => "outflow", "status" => "success"},
    )
    .await;

  helper.shutdown().await;
}

#[tokio::test]
async fn default_config_invalid_lines() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:tcp"], &[]).await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let helper = Helper::new(
    &fmt!(
      DEFAULT_CONFIG,
      bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
  )
  .await;
  let mut stream = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp"))
    .await
    .unwrap();

  let lines = &[
    "node_request.count 1001 source=server-0\n",
    "node_system.cpu.load\\# 0.03 source=server-0\n", /* invalid: Metric name has an invalid
                                                       * character (‘#’) */
    "node_system.cpu.loadavg.1m 0.03 1382754475 source=server-1\n",
    "node_system.cpu.loadavg source=server-0\n", // invalid: No metric value
    "node_marketing.adsense.impressions 24056 source=campaign1\n",
  ];

  write_all(&mut stream, lines).await;

  assert_eq!(
    clean_timestamps(parse_carbon_metrics(&[lines[0], lines[2], lines[4]])),
    clean_timestamps(upstream.wait_for_metrics().await)
  );

  helper.shutdown().await;
}

#[tokio::test]
async fn default_config_elision() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:tcp"], &[]).await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let helper = Helper::new(
    &fmt!(
      DEFAULT_CONFIG,
      bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
  )
  .await;
  let mut stream = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp"))
    .await
    .unwrap();

  // 0, 9, and 19 should be reported.
  let current_time = current_time();
  let lines = (0 .. 20)
    .map(|i| {
      format!(
        "node_request.count 0 {} source=server-0\n",
        current_time + i
      )
    })
    .collect_vec();

  write_all(
    &mut stream,
    &lines.iter().map(std::string::String::as_str).collect_vec(),
  )
  .await;

  assert_eq!(
    parse_carbon_metrics(&[&lines[0], &lines[9], &lines[19]]),
    upstream.wait_for_metrics().await
  );

  helper.shutdown().await;
}

#[tokio::test]
async fn default_config_edges() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:tcp"], &[]).await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let helper = Helper::new(
    &fmt!(
      DEFAULT_CONFIG,
      bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
  )
  .await;
  let mut stream = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp"))
    .await
    .unwrap();

  let current_time = current_time();
  // First element, the non-zero element, and the transition back to zero should be reported.
  let lines = (0 .. 16)
    .map(|i| {
      if i == 7 {
        format!(
          "node_request.count 10.1 {} source=server-0\n",
          current_time + i
        )
      } else {
        format!(
          "node_request.count 0 {} source=server-0\n",
          current_time + i
        )
      }
    })
    .collect_vec();

  write_all(
    &mut stream,
    &lines.iter().map(std::string::String::as_str).collect_vec(),
  )
  .await;

  assert_eq!(
    parse_carbon_metrics(&[&lines[0], &lines[7], &lines[8]]),
    upstream.wait_for_metrics().await
  );

  helper.shutdown().await;
}

fmt_reuse! {
ANALYZE_CONFIG = r#"
pipeline:
  inflows:
    tcp:
      routes: ["processor:populate_cache"]
      tcp:
        bind: "inflow:tcp"
        protocol:
          carbon: {{}}

  processors:
    populate_cache:
      routes: ["processor:elision"]
      populate_cache: {{}}

    elision:
      routes: ["outflow:tcp"]
      elision:
        emit: {{ ratio: 0.1 }}
        analyze_mode: true

  outflows:
    tcp:
      tcp:
        common:
          send_to: "{}"
          protocol:
            carbon: {{}}
"#;
}

#[tokio::test]
async fn analyze_config() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:tcp"], &[]).await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let helper = Helper::new(
    &fmt!(
      ANALYZE_CONFIG,
      bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
  )
  .await;
  let mut stream = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp"))
    .await
    .unwrap();

  // Should not elide any metrics.
  let current_time = current_time();
  let lines = (0 .. 16)
    .flat_map(|i| {
      let value = if i == 8 { 10.1 } else { 0.0 };
      vec![
        format!(
          "node_runtime.memory.usage {value} {} source=server-0\n",
          current_time + i
        ),
        format!(
          "node_runtime.cpu.usage {value} {} source=server-0\n",
          current_time + i
        ),
      ]
    })
    .collect_vec();

  write_all(
    &mut stream,
    &lines.iter().map(std::string::String::as_str).collect_vec(),
  )
  .await;

  assert_eq!(
    parse_carbon_metrics(&lines.iter().map(std::string::String::as_str).collect_vec()),
    upstream.wait_for_metrics().await
  );

  helper.shutdown().await;
}

fmt_reuse! {
FREQUENCY_CONFIG = r#"
pipeline:
  inflows:
    tcp:
      routes: ["processor:populate_cache"]
      tcp:
        bind: "inflow:tcp"
        protocol:
          carbon: {{}}

  processors:
    populate_cache:
      routes: ["processor:elision"]
      populate_cache: {{}}

    elision:
      routes: ["outflow:tcp"]
      elision:
        emit: {{ ratio: 0.5 }}

  outflows:
    tcp:
      tcp:
        common:
          send_to: "{}"
          protocol:
            carbon: {{}}
"#;
}

#[tokio::test]
async fn frequency_config() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:tcp"], &[]).await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let helper = Helper::new(
    &fmt!(
      FREQUENCY_CONFIG,
      bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
  )
  .await;
  let mut stream = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp"))
    .await
    .unwrap();

  // First seen should be reported, then alternating between reported and not.
  let current_time = current_time();
  let lines = (0 .. 21)
    .map(|i| {
      format!(
        "node_request.count 0 {} source=server-0\n",
        current_time + i
      )
    })
    .collect_vec();

  write_all(
    &mut stream,
    &lines.iter().map(std::string::String::as_str).collect_vec(),
  )
  .await;

  let mut expected = vec![lines[0].as_str()];
  expected.extend((1 .. 20).step_by(2).map(|i| lines[i].as_str()));

  assert_eq!(
    parse_carbon_metrics(&expected),
    upstream.wait_for_metrics().await
  );

  helper.shutdown().await;
}

fmt_reuse! {
REGEX_OVERRIDE_CONFIG = r#"
pipeline:
  inflows:
    tcp:
      routes: ["processor:populate_cache"]
      tcp:
        bind: "inflow:tcp"
        protocol:
          carbon: {{}}

  processors:
    populate_cache:
      routes: ["processor:elision"]
      populate_cache: {{}}

    elision:
      routes: ["outflow:tcp"]
      elision:
        emit: {{ ratio: 0.1 }}
        regex_overrides:
          - regex: "^node_request.*"
            emit: {{ ratio: 0.5 }}
          - regex: "^prod.*"
            emit: {{ ratio: 0.25 }}

  outflows:
    tcp:
      tcp:
        common:
          send_to: "{}"
          protocol:
            carbon: {{}}
"#;
}

#[tokio::test]
async fn regex_override_config() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:tcp"], &[]).await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let helper = Helper::new(
    &fmt!(
      REGEX_OVERRIDE_CONFIG,
      bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
  )
  .await;
  let mut stream = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp"))
    .await
    .unwrap();

  // First seen should be reported, then alternating between reported and not.
  let current_time = current_time();
  let lines = (0 .. 5)
    .map(|i| {
      format!(
        "node_request.count 0 {} source=server-0\n",
        current_time + i
      )
    })
    .collect_vec();

  write_all(
    &mut stream,
    &lines.iter().map(std::string::String::as_str).collect_vec(),
  )
  .await;

  assert_eq!(
    parse_carbon_metrics(&[&lines[0], &lines[1], &lines[3]]),
    upstream.wait_for_metrics().await
  );

  helper.shutdown().await;
}

fmt_reuse! {
EMIT_INTERVAL_CONFIG = r#"
pipeline:
  inflows:
    tcp:
      routes: ["processor:populate_cache"]
      tcp:
        bind: "inflow:tcp"
        protocol:
          carbon: {{}}

  processors:
    populate_cache:
      routes: ["processor:elision"]
      populate_cache: {{}}

    elision:
      routes: ["outflow:tcp"]
      elision:
        emit: {{ interval: 60s }}

  outflows:
    tcp:
      tcp:
        common:
          send_to: "{}"
          protocol:
            carbon: {{}}
"#;
}

#[tokio::test]
async fn emit_interval_config() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:tcp"], &[]).await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let helper = Helper::new(
    &fmt!(
      EMIT_INTERVAL_CONFIG,
      bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
  )
  .await;
  let mut stream = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp"))
    .await
    .unwrap();

  let current_time = current_time();
  let lines = vec![
    format!(
      "node_request.count 0 {} source=server-0\n",
      current_time + 100
    ), // first seen, reported
    format!(
      "node_request.count 0 {} source=server-0\n",
      current_time + 130
    ), // elided
    format!(
      "node_request.count 0 {} source=server-0\n",
      current_time + 159
    ), // elided
    format!(
      "node_request.count 0 {} source=server-0\n",
      current_time + 165
    ), // reported
    format!(
      "node_request.count 0 {} source=server-0\n",
      current_time + 220
    ), // elided
    format!(
      "node_request.count 0 {} source=server-0\n",
      current_time + 222
    ), // elided
    format!(
      "node_request.count 0 {} source=server-0\n",
      current_time + 224
    ), // elided
    format!(
      "node_request.count 0 {} source=server-0\n",
      current_time + 225
    ), // reported
    format!(
      "node_request.count 5 {} source=server-0\n",
      current_time + 230
    ), // reported
    format!(
      "node_request.count 0 {} source=server-0\n",
      current_time + 240
    ), // reported
    format!(
      "node_request.count 0 {} source=server-0\n",
      current_time + 299
    ), // elided
    format!(
      "node_request.count 0 {} source=server-0\n",
      current_time + 300
    ), // reported
    format!(
      "node_request.count 0 {} source=server-0\n",
      current_time + 400
    ), // reported
  ];

  write_all(
    &mut stream,
    &lines.iter().map(std::string::String::as_str).collect_vec(),
  )
  .await;

  assert_eq!(
    parse_carbon_metrics(&[
      &lines[0], &lines[3], &lines[7], &lines[8], &lines[9], &lines[11], &lines[12]
    ]),
    upstream.wait_for_metrics().await
  );

  helper.shutdown().await;
}
