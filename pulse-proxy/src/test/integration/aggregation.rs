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
use pretty_assertions::assert_eq;
use pulse_metrics::test::{clean_timestamps, parse_carbon_metrics};
use reusable_fmt::{fmt, fmt_reuse};
use tokio::net::TcpStream;

// TODO(mattklein123): Add more integration tests including prometheus remote write w/
// absolute counters.
// TODO(mattklein123): Fake time does not work well with network timeouts. If we want to use fake
// time we are going to have to allow disabling all relevant timeouts.

fmt_reuse! {
AGGREGATION = r#"
pipeline:
  inflows:
    tcp:
      routes: ["processor:populate_cache"]
      tcp:
        bind: "inflow:tcp"
        protocol:
          statsd: {{}}

  processors:
    populate_cache:
      routes: ["processor:aggregation"]
      populate_cache: {{}}

    aggregation:
      routes: ["outflow:tcp"]
      aggregation:
        flush_interval: 1s
        quantile_timers:
          extended:
            mean: true
            count: true

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
async fn aggregation() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:tcp"], &[]).await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let helper = Helper::new(
    &fmt!(
      AGGREGATION,
      fake_upstream = bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
  )
  .await;
  let mut stream = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp"))
    .await
    .unwrap();

  write_all(&mut stream, &["hello:5|c\n", "world:45|ms\n"]).await;

  assert_eq!(
    parse_carbon_metrics(&[
      "hello 5 0",
      "world.mean 45 0",
      "world.count 1 0",
      "world.p50 45 0",
      "world.p95 45 0",
      "world.p99 45 0"
    ]),
    clean_timestamps(upstream.wait_for_metrics().await)
  );

  helper.shutdown().await;
}

fmt_reuse! {
TWO_AGGREGATIONS = r#"
admin:
  bind: admin

pipeline:
  inflows:
    tcp:
      routes: ["processor:populate_cache"]
      tcp:
        bind: "inflow:tcp"
        protocol:
          statsd: {{}}

  processors:
    populate_cache:
      routes: ["processor:aggregation", "processor:per_instance_filter"]
      populate_cache: {{}}

    aggregation:
      routes: ["outflow:tcp_agg"]
      aggregation:
        flush_interval: 1s
        enable_last_aggregation_admin_endpoint: true
        reservoir_timers: {{}}

    per_instance_filter:
      routes: ["processor:per_instance_aggregation"]
      regex:
        allow:
          - "^instance\\..*"

    per_instance_aggregation:
      routes: ["outflow:tcp_instance"]
      aggregation:
        flush_interval: 1s
        pin_flush_interval_to_wall_clock: false
        enable_last_aggregation_admin_endpoint: true
        quantile_timers:
          extended:
            mean: true
            count: true

  outflows:
    tcp_instance:
      tcp:
        common:
          send_to: "{fake_upstream_instance}"
          protocol:
            carbon: {{}}

    tcp_agg:
      tcp:
        common:
          send_to: "{fake_upstream_agg}"
          protocol:
            carbon: {{}}
  "#;
}

#[tokio::test]
async fn multiple_aggregation() {
  let bind_resolver = HelperBindResolver::new(
    &[
      "admin",
      "fake_upstream_instance",
      "fake_upstream_agg",
      "inflow:tcp",
    ],
    &[],
  )
  .await;
  let mut upstream_agg = FakeWireUpstream::new("fake_upstream_agg", bind_resolver.clone()).await;
  let mut upstream_instance =
    FakeWireUpstream::new("fake_upstream_instance", bind_resolver.clone()).await;
  let helper = Helper::new(
    &fmt!(
      TWO_AGGREGATIONS,
      fake_upstream_instance = bind_resolver.local_tcp_addr("fake_upstream_instance"),
      fake_upstream_agg = bind_resolver.local_tcp_addr("fake_upstream_agg"),
    ),
    bind_resolver.clone(),
  )
  .await;
  let mut stream = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp"))
    .await
    .unwrap();

  write_all(
    &mut stream,
    &[
      "instance.hello:5|c\n",
      "instance.world:45|ms\n",
      "blah:7|g\n",
    ],
  )
  .await;

  assert_eq!(
    parse_carbon_metrics(&[
      "instance.hello 5 0",
      "instance.world.mean 45 0",
      "instance.world.count 1 0",
      "instance.world.p50 45 0",
      "instance.world.p95 45 0",
      "instance.world.p99 45 0"
    ]),
    clean_timestamps(upstream_instance.wait_for_metrics().await)
  );

  assert_eq!(
    parse_carbon_metrics(&["instance.hello 5 0", "instance.world 45 0", "blah 7 0",]),
    clean_timestamps(upstream_agg.wait_for_metrics().await)
  );

  // The filters can get dumped in any order so we need to account for that.
  let response =
    make_admin_request(bind_resolver.local_tcp_addr("admin"), "/last_aggregation").await;
  let response = response.trim();

  assert!(response.contains(
    r"dumping filter: aggregation
instance.hello()
instance.world()
blah()"
  ));

  assert!(response.contains(
    r"dumping filter: per_instance_aggregation
instance.hello()
instance.world.mean()
instance.world.count()
instance.world.p50()
instance.world.p95()
instance.world.p99()"
  ));

  helper.shutdown().await;
}
