// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::test::integration::{FakePromUpstream, FakeWireUpstream, Helper, HelperBindResolver};
use pretty_assertions::assert_eq;
use prom_remote_write::prom_remote_write_server_config::ParseConfig;
use pulse_metrics::test::{clean_timestamps, make_abs_counter, make_tag};
use pulse_protobuf::protos::pulse::config::inflow::v1::prom_remote_write;
use reusable_fmt::{fmt, fmt_reuse};

fmt_reuse! {
META_STATS = r#"
meta_stats:
  flush_interval: 1s
  meta_protocol:
    - wire:
        common:
          send_to: "{fake_upstream}"
          protocol:
            carbon: {{}}

pipeline:
  inflows:
    tcp:
      routes: ["outflow:tcp"]
      tcp:
        bind: "inflow:tcp"
        protocol:
          carbon: {{}}

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
async fn meta_stats() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:tcp"], &[]).await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let helper = Helper::new(
    &fmt!(
      META_STATS,
      fake_upstream = bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
  )
  .await;

  let metric = upstream.wait_for_metrics().await;
  assert!(metric[0]
    .metric()
    .get_id()
    .name()
    .starts_with(b"pulse_proxy."));

  helper.shutdown().await;
}

fmt_reuse! {
PREFIX = r#"
meta_stats:
  flush_interval: 1s
  node_id: {{ inline: "node-1" }}
  meta_prefix: "production:app:pulse"
  meta_protocol:
    - wire:
        common:
          send_to: "{fake_upstream}"
          protocol:
            carbon: {{}}

pipeline:
  inflows:
    tcp:
      routes: ["outflow:tcp"]
      tcp:
        bind: "inflow:tcp"
        protocol:
          carbon: {{}}

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
async fn prefix() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:tcp"], &[]).await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let helper = Helper::new(
    &fmt!(
      PREFIX,
      fake_upstream = bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
  )
  .await;

  let metric = upstream.wait_for_metrics().await;
  assert!(metric[0]
    .metric()
    .get_id()
    .name()
    .starts_with(b"production.app.pulse."));
  assert_eq!(
    metric[0].metric().get_id().tags()[0],
    make_tag("source", "node-1")
  );

  helper.shutdown().await;
}

fmt_reuse! {
PROM = r#"
  meta_stats:
    flush_interval: 1s
    node_id: {{ inline: "node-1" }}
    meta_prefix: "production:app:pulse"
    meta_protocol:
      - prom_remote_write:
          send_to: "http://{fake_upstream}/api/v1/prom/write"

  pipeline:
    inflows:
      tcp:
        routes: ["outflow:prom_remote_write"]
        tcp:
          bind: "inflow:tcp"
          protocol:
            carbon: {{}}

    outflows:
      prom_remote_write:
        prom_remote_write:
          send_to: "http://{fake_upstream}/api/v1/prom/write"
  "#;
  }

#[tokio::test]
async fn prom() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:tcp"], &[]).await;
  let mut upstream = FakePromUpstream::new(
    "fake_upstream",
    bind_resolver.clone(),
    ParseConfig::default(),
  )
  .await;
  let helper = Helper::new(
    &fmt!(
      PROM,
      fake_upstream = bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
  )
  .await;

  let (_, metrics) = upstream.wait_for_metrics().await;
  assert_eq!(
    clean_timestamps(metrics)[0],
    make_abs_counter(
      "production:app:pulse:config:failed_update",
      &[("source", "node-1")],
      0,
      0.0
    )
  );

  helper.shutdown().await;
}
