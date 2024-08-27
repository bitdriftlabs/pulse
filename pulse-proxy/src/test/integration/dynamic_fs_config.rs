// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::test::integration::{FakePromUpstream, Helper, HelperBindResolver, PromClient};
use pretty_assertions::assert_eq;
use prom_remote_write::prom_remote_write_server_config::ParseConfig;
use prometheus::labels;
use pulse_metrics::test::{make_abs_counter, FsConfigSwapHelper};
use pulse_protobuf::protos::pulse::config::inflow::v1::prom_remote_write;
use reusable_fmt::{fmt, fmt_reuse};

fmt_reuse! {
DYNAMIC_FS_BOOTSTRAP = r#"
fs_watched_pipeline:
  dir: "{config_dir}"
  file: "{config_dir}/config.yaml"
  "#;
}

fmt_reuse! {
DYNAMIC_INFLOW_1 = r#"
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
    routes: ["outflow:prom_remote_write"]
    elision:
      emit: {{ ratio: 0.1 }}

outflows:
  prom_remote_write:
    prom_remote_write:
      send_to: "http://{fake_upstream}/api/v1/prom/write"
"#;
}

fmt_reuse! {
DYNAMIC_INFLOW_2 = r#"
inflows:
  prom_remote_write:
    routes: ["processor:buffer"]
    prom_remote_write:
      bind: "inflow:prom"
      parse_config:
        ignore_duplicate_metadata: true

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
    routes: ["outflow:prom_remote_write"]
    elision:
      emit: {{ ratio: 0.1 }}

outflows:
  prom_remote_write:
    prom_remote_write:
      send_to: "http://{fake_upstream}/api/v1/prom/write"
  "#;
}

#[tokio::test]
async fn reload_config() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:prom"], &[]).await;
  let mut upstream = FakePromUpstream::new(
    "fake_upstream",
    bind_resolver.clone(),
    ParseConfig::default(),
  )
  .await;
  let mut config_helper = FsConfigSwapHelper::new(&fmt!(
    DYNAMIC_INFLOW_1,
    fake_upstream = bind_resolver.local_tcp_addr("fake_upstream")
  ));
  let helper = Helper::new(
    &fmt!(
      DYNAMIC_FS_BOOTSTRAP,
      config_dir = config_helper.path().display()
    ),
    bind_resolver.clone(),
  )
  .await;
  let client = PromClient::new(bind_resolver.local_tcp_addr("inflow:prom")).await;

  client.send(&[make_abs_counter("hello", &[], 1, 1.0)]).await;
  assert_eq!(
    vec![make_abs_counter("hello", &[], 1, 1.0)],
    upstream.wait_for_metrics().await.1
  );

  config_helper.update_config(&fmt!(
    DYNAMIC_INFLOW_2,
    fake_upstream = bind_resolver.local_tcp_addr("fake_upstream")
  ));
  helper
    .stats_helper()
    .wait_for_counter_eq(1, "pulse_proxy:config:updated", &labels! {})
    .await;

  // Use a fresh client as the previous client's connection may race with the old inflow shutdown.
  let client = PromClient::new(bind_resolver.local_tcp_addr("inflow:prom")).await;
  client.send(&[make_abs_counter("world", &[], 1, 1.0)]).await;
  assert_eq!(
    vec![make_abs_counter("world", &[], 1, 1.0)],
    upstream.wait_for_metrics().await.1
  );

  helper.shutdown().await;
}

#[tokio::test]
async fn bad_config() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:prom"], &[]).await;
  let mut upstream = FakePromUpstream::new(
    "fake_upstream",
    bind_resolver.clone(),
    ParseConfig::default(),
  )
  .await;
  let mut config_helper = FsConfigSwapHelper::new(&fmt!(
    DYNAMIC_INFLOW_1,
    fake_upstream = bind_resolver.local_tcp_addr("fake_upstream")
  ));
  let helper = Helper::new(
    &fmt!(
      DYNAMIC_FS_BOOTSTRAP,
      config_dir = config_helper.path().display()
    ),
    bind_resolver.clone(),
  )
  .await;
  let client = PromClient::new(bind_resolver.local_tcp_addr("inflow:prom")).await;

  config_helper.update_config("bad config");
  helper
    .stats_helper()
    .wait_for_counter_eq(1, "pulse_proxy:config:failed_update", &labels! {})
    .await;

  client.send(&[make_abs_counter("world", &[], 1, 1.0)]).await;
  assert_eq!(
    vec![make_abs_counter("world", &[], 1, 1.0)],
    upstream.wait_for_metrics().await.1
  );

  helper.shutdown().await;
}
