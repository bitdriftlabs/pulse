// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::test::integration::{
  FakeWireUpstream,
  Helper,
  HelperBindResolver,
  make_admin_request,
  write_all,
};
use itertools::Itertools;
use pretty_assertions::assert_eq;
use prometheus::labels;
use pulse_metrics::test::parse_carbon_metrics;
use reusable_fmt::{fmt, fmt_reuse};
use std::sync::Arc;
use tokio::net::TcpStream;

fmt_reuse! {
BASIC = r#"
admin:
  bind: "{admin_bind}"

pipeline:
  inflows:
    tcp:
      routes: ["processor:internode"]
      tcp:
        bind: "{inflow_bind}"
        protocol:
          carbon: {{}}

  processors:
    populate_cache:
      routes: ["processor:elision"]
      populate_cache: {{}}

    internode:
      alt_routes: ["outflow:tcp"]
      routes: ["processor:populate_cache"]
      internode:
        listen: "{internode_bind}"
        total_nodes: 2
        this_node_id: "{internode_name}"
        nodes:
          - {{ node_id: "{other_internode_name}", address: "{other_internode_addr}" }}

    elision:
      routes: ["outflow:tcp"]
      elision:
        emit: {{ ratio: 0.1 }}

  outflows:
    tcp:
      tcp:
        common:
          send_to: "{outflow_addr}"
          protocol:
            carbon: {{}}
"#;
}

async fn start_multi_node() -> (Arc<HelperBindResolver>, Helper, Helper) {
  let bind_resolver = HelperBindResolver::new(
    &[
      "admin1",
      "admin2",
      "fake_upstream1",
      "fake_upstream2",
      "inflow:tcp_1",
      "inflow:tcp_2",
      "processor:internode_1",
      "processor:internode_2",
    ],
    &[],
  )
  .await;

  let helper1 = Helper::new(
    &fmt!(
      BASIC,
      admin_bind = "admin1",
      inflow_bind = "inflow:tcp_1",
      internode_bind = "processor:internode_1",
      internode_name = "node-1",
      other_internode_name = "node-2",
      other_internode_addr = bind_resolver.local_tcp_addr("processor:internode_2"),
      outflow_addr = bind_resolver.local_tcp_addr("fake_upstream1")
    ),
    bind_resolver.clone(),
  )
  .await;
  let helper2 = Helper::new(
    &fmt!(
      BASIC,
      admin_bind = "admin2",
      inflow_bind = "inflow:tcp_2",
      internode_bind = "processor:internode_2",
      internode_name = "node-2",
      other_internode_name = "node-1",
      other_internode_addr = bind_resolver.local_tcp_addr("processor:internode_1"),
      outflow_addr = bind_resolver.local_tcp_addr("fake_upstream2")
    ),
    bind_resolver.clone(),
  )
  .await;

  (bind_resolver, helper1, helper2)
}

#[tokio::test]
async fn basic() {
  let (bind_resolver, helper1, helper2) = start_multi_node().await;
  let mut upstream1 = FakeWireUpstream::new("fake_upstream1", bind_resolver.clone()).await;
  let mut stream1 = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp_1"))
    .await
    .unwrap();
  let mut stream2 = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp_2"))
    .await
    .unwrap();

  let lines = &[
    "node_request.count 1 0 source=server-0\n",
    "node_request.count 1 1 source=server-0\n",
    "node_request.count 1 2 source=server-0\n",
  ];

  for (i, item) in lines.iter().enumerate() {
    if i % 2 == 0 {
      write_all(&mut stream1, &[item]).await;
    } else {
      write_all(&mut stream2, &[item]).await;
    }
  }

  let mut metrics = upstream1.wait_for_metrics().await;
  metrics.sort_by(|m1, m2| m1.metric().timestamp.cmp(&m2.metric().timestamp));
  assert_eq!(parse_carbon_metrics(lines), metrics);

  helper1.shutdown().await;
  helper2.shutdown().await;
}

#[tokio::test]
async fn elision() {
  let (bind_resolver, helper1, helper2) = start_multi_node().await;
  let mut upstream1 = FakeWireUpstream::new("fake_upstream1", bind_resolver.clone()).await;
  let mut stream1 = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp_1"))
    .await
    .unwrap();
  let mut stream2 = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp_2"))
    .await
    .unwrap();

  // 0, 9, and 19 should be reported.
  let current_time = 1;
  let lines = (0 .. 20)
    .map(|i| {
      format!(
        "node_request.count 0 {} source=server-0\n",
        current_time + i
      )
    })
    .collect_vec();

  // To avoid out of order samples we need to wait for every sample to be received before sending
  // the next one.
  for (i, line) in lines.iter().enumerate() {
    if i % 2 == 0 {
      write_all(&mut stream1, &[line]).await;
    } else {
      write_all(&mut stream2, &[line]).await;
    }

    helper1
      .stats_helper()
      .wait_for_counter_eq(
        i as u64 + 1,
        "pulse_proxy:pipeline:messages_routed",
        &labels! {
          "src" => "internode",
          "src_type" => "processor",
          "dest" => "populate_cache",
          "dest_type" => "processor",
        },
      )
      .await;
  }

  assert_eq!(
    parse_carbon_metrics(&[&lines[0], &lines[9], &lines[19]]),
    upstream1.wait_for_num_metrics(3).await
  );

  assert_eq!(
    (current_time + 18).to_string(),
    make_admin_request(
      bind_resolver.local_tcp_addr("admin1"),
      "/last_elided?metric=node_request.count"
    )
    .await
  );
  assert_eq!(
    (current_time + 18).to_string(),
    make_admin_request(
      bind_resolver.local_tcp_addr("admin2"),
      "/last_elided?metric=node_request.count"
    )
    .await
  );

  helper1.shutdown().await;
  helper2.shutdown().await;
}

#[tokio::test]
async fn failure() {
  let (bind_resolver, helper1, helper2) = start_multi_node().await;
  let mut upstream1 = FakeWireUpstream::new("fake_upstream1", bind_resolver.clone()).await;
  let mut upstream2 = FakeWireUpstream::new("fake_upstream2", bind_resolver.clone()).await;
  let mut stream2 = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp_2"))
    .await
    .unwrap();

  let lines = &[
    "node_request.count 0 0 source=server-0\n",
    "node_request.count 0 1 source=server-0\n",
    "node_request.count 0 2 source=server-0\n",
    "node_request.count 0 3 source=server-0\n",
  ];
  let lines_after_failure = &[
    "node_request.count 0 4 source=server-0\n",
    "node_request.count 0 5 source=server-0\n",
    "node_request.count 0 6 source=server-0\n",
    "node_request.count 0 7 source=server-0\n",
    "node_request.count 0 8 source=server-0\n",
    "node_request.count 0 9 source=server-0\n",
  ];

  // The metric is assigned to 1. Send some traffic to 2 that should get deferred to 1 and elided.
  write_all(&mut stream2, lines).await;
  assert_eq!(
    parse_carbon_metrics(&[lines[0]]),
    upstream1.wait_for_metrics().await
  );
  helper1.shutdown().await;

  // Send remaining traffic to 2. Since instance 1 was terminated, 2 should just forward the lines
  // to the sink without eliding.
  write_all(&mut stream2, lines_after_failure).await;
  assert_eq!(
    parse_carbon_metrics(lines_after_failure),
    upstream2.wait_for_metrics().await
  );

  helper2.shutdown().await;
}

#[tokio::test]
async fn mixed_points() {
  let (bind_resolver, helper1, helper2) = start_multi_node().await;
  let mut upstream1 = FakeWireUpstream::new("fake_upstream1", bind_resolver.clone()).await;
  let mut upstream2 = FakeWireUpstream::new("fake_upstream2", bind_resolver.clone()).await;
  let mut stream1 = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp_2"))
    .await
    .unwrap();
  let mut stream2 = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp_2"))
    .await
    .unwrap();

  let lines = &[
    "node_request.count 123 0 source=server-0\n",
    "node_request.500.sum 234 0 source=server-0\n",
    "node_request.failure.count 25 1 source=server-0\n",
    "node_request.count 120 1 source=server-0\n",
    "node_request.500.sum 0 2 source=server-0\n",
    "node_request.500.sum 20 3 source=server-0\n",
    "node_request.count 120 2 source=server-0\n",
    "node_request.other_request.timer.p95 12345.938 3 source=server-0\n",
    "node_request.other_request.timer.p95 5.938 4 source=server-0\n",
    "node_request.count 130 4 source=server-0\n",
    "node_request.count 140 5 source=server-0\n",
    "node_request.other_request.timer.p95 1E-6 5 source=server-0\n",
    "node_request.failure.count 35 6 source=server-0\n",
    "node_request.count 15 7 source=server-0\n",
  ];

  for (i, line) in lines.iter().enumerate() {
    if i % 2 == 0 {
      write_all(&mut stream1, &[line]).await;
    } else {
      write_all(&mut stream2, &[line]).await;
    }
  }

  let mut metrics = upstream1.wait_for_metrics().await;
  metrics.extend(upstream2.wait_for_metrics().await);
  // TODO(mattklein123): Actually compare the metrics above.
  assert_eq!(lines.len(), metrics.len());

  helper1.shutdown().await;
  helper2.shutdown().await;
}
