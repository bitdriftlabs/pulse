// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::test::integration::{FakeWireUpstream, Helper, HelperBindResolver, write_all};
use pretty_assertions::assert_eq;
use pulse_metrics::test::parse_carbon_metrics;
use reusable_fmt::{fmt, fmt_reuse};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::sleep;

fmt_reuse! {
INSTANCE_A = r#"
pipeline:
  inflows:
    tcp:
      routes: ["processor:populate_cache"]
      tcp:
        bind: "inflow:tcp_a"
        protocol:
          carbon: {{}}

  processors:
    populate_cache:
      routes: ["processor:elision"]
      populate_cache: {{}}

    elision:
      routes: ["outflow:tcp"]
      elision:
        analyze_mode: true
        emit: {{ ratio: 0.1 }}

  outflows:
    tcp:
      tcp:
        common:
          send_to: "{instance_b}"
          write_timeout: 0.25s
          protocol:
            carbon: {{}}
"#;

INSTANCE_B = r#"
pipeline:
  inflows:
    tcp:
      routes: ["processor:populate_cache"]
      tcp:
        bind: "inflow:tcp_b"
        protocol:
          carbon: {{}}
        advanced: {{ idle_timeout: 0.5s }}

  processors:
    populate_cache:
      routes: ["processor:elision"]
      populate_cache: {{}}

    elision:
      routes: ["outflow:tcp"]
      elision:
        analyze_mode: true
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
async fn socket_close() {
  let bind_resolver =
    HelperBindResolver::new(&["fake_upstream", "inflow:tcp_a", "inflow:tcp_b"], &[]).await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let helper_a = Helper::new(
    &fmt!(
      INSTANCE_A,
      instance_b = bind_resolver.local_tcp_addr("inflow:tcp_b")
    ),
    bind_resolver.clone(),
  )
  .await;
  let helper_b = Helper::new(
    &fmt!(
      INSTANCE_B,
      fake_upstream = bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
  )
  .await;
  let mut stream = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp_a"))
    .await
    .unwrap();

  let lines = &[
    "node_request.count 500 10 source=server-0\n",
    "node_request.count 500 12 source=server-0\n",
  ];

  write_all(&mut stream, &[lines[0]]).await;
  // Wait for idle_timeout to be triggered on B
  sleep(Duration::from_millis(750)).await;
  write_all(&mut stream, &[lines[1]]).await;

  assert_eq!(
    parse_carbon_metrics(lines),
    upstream.wait_for_num_metrics(2).await
  );

  helper_a.shutdown().await;
  helper_b.shutdown().await;
}
