// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::test::integration::{
  FakeRemoteFileSource,
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
use time::macros::datetime;
use tokio::net::TcpStream;

fmt_reuse! {
REMOTE_FETCH_CACHING = r#"
pipeline:
  inflows:
    tcp:
      routes: ["processor:populate_cache"]
      tcp:
        bind: "{inflow_tcp}"
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
        blocklist:
          blocked_metrics:
            http:
              url: "http://{fake_file_source}/blocklist.fst"
              interval: 1s
              request_timeout: 1s
              cache_path: {cache_dir}/blocklist.fst

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
async fn file_caching() {
  let bind_resolver = HelperBindResolver::new(
    &[
      "fake_upstream",
      "fake_file_source",
      "inflow:tcp_1",
      "inflow:tcp_2",
    ],
    &[],
  )
  .await;
  let file_source = FakeRemoteFileSource::new("fake_file_source", bind_resolver.clone()).await;
  file_source.update_file(
    "/blocklist.fst",
    FakeRemoteFileSource::make_fst(&["hello", "world", "barbaz"]),
  );
  let temp_dir = tempfile::tempdir().unwrap();

  let helper = Helper::new(
    &fmt!(
      REMOTE_FETCH_CACHING,
      inflow_tcp = "inflow:tcp_1",
      fake_upstream = bind_resolver.local_tcp_addr("fake_upstream"),
      fake_file_source = bind_resolver.local_tcp_addr("fake_file_source"),
      cache_dir = temp_dir.path().display(),
    ),
    bind_resolver.clone(),
  )
  .await;

  helper
    .stats_helper()
    .wait_for_counter_eq(
      1,
      "pulse_proxy:pipeline:processor:elision:filter:poll:load_total",
      &labels! { "filter_name" => "blocklist"},
    )
    .await;

  helper.shutdown().await;
  file_source.remove_file("/blocklist.fst");

  let helper = Helper::new(
    &fmt!(
      REMOTE_FETCH_CACHING,
      inflow_tcp = "inflow:tcp_2",
      fake_upstream = bind_resolver.local_tcp_addr("fake_upstream"),
      fake_file_source = bind_resolver.local_tcp_addr("fake_file_source"),
      cache_dir = temp_dir.path().display(),
    ),
    bind_resolver.clone(),
  )
  .await;

  let mut stream = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp_2"))
    .await
    .unwrap();

  write_all(
    &mut stream,
    &[
      "hello 1 0 source=server-0\n",
      "hello 1 1 source=server-0\n",
      "world 1 0 source=server-0\n",
      "world 1 1 source=server-0\n",
      "barbaz 1 0 source=server-0\n",
      "barbaz 1 1 source=server-0\n",
    ],
  )
  .await;

  helper
    .stats_helper()
    .wait_for_counter_eq(
      6,
      "pulse_proxy:pipeline:processor:elision:filter:considered",
      &labels! { "filter_name" => "blocklist", "match_decision" => "fail"},
    )
    .await;

  helper.shutdown().await;
}

fmt_reuse! {
BLOCKLISTING_REMOTE_FETCH = r#"
admin:
  bind: admin

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
        zero:
          enabled: false
        blocklist:
          allowed_metric_patterns:
            http:
              url: "http://{fake_file_source}/allowed_metric_patterns.csv"
              interval: 1s
              request_timeout: 1s
          allowed_metrics:
            http:
              url: "http://{fake_file_source}/allowed_metrics.fst"
              interval: 1s
              request_timeout: 1s
          blocked_metrics:
            http:
              url: "http://{fake_file_source}/blocklist.fst"
              interval: 1s
              request_timeout: 1s

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
async fn blocklisting_remote_fetch() {
  let bind_resolver = HelperBindResolver::new(
    &["admin", "fake_upstream", "fake_file_source", "inflow:tcp"],
    &[],
  )
  .await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let file_source = FakeRemoteFileSource::new("fake_file_source", bind_resolver.clone()).await;
  file_source.update_file(
    "/allowed_metrics.fst",
    FakeRemoteFileSource::make_fst(&["hello"]),
  );
  file_source.update_file(
    "/allowed_metric_patterns.csv",
    FakeRemoteFileSource::make_regex_csv(&["bar.*"]),
  );
  file_source.update_file(
    "/blocklist.fst",
    FakeRemoteFileSource::make_fst(&["hello", "world", "barbaz"]),
  );
  let helper = Helper::new(
    &fmt!(
      BLOCKLISTING_REMOTE_FETCH,
      fake_upstream = bind_resolver.local_tcp_addr("fake_upstream"),
      fake_file_source = bind_resolver.local_tcp_addr("fake_file_source"),
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
      "hello 1 0 source=server-0\n",
      "hello 1 1 source=server-0\n",
      "world 1 0 source=server-0\n",
      "world 1 1 source=server-0\n",
      "barbaz 1 0 source=server-0\n",
      "barbaz 1 1 source=server-0\n",
    ],
  )
  .await;

  assert_eq!(
    parse_carbon_metrics(&[
      "hello 1 0 source=server-0\n",
      "hello 1 1 source=server-0\n",
      "barbaz 1 0 source=server-0\n",
      "barbaz 1 1 source=server-0\n",
    ]),
    upstream.wait_for_metrics().await
  );

  assert_eq!(
    r"
filter name: exact_allowlist
hello

filter name: regex_allowlist
bar.*

filter name: blocklist
barbaz
hello
world
    "
    .trim(),
    make_admin_request(bind_resolver.local_tcp_addr("admin"), "/dump_poll_filter")
      .await
      .trim()
  );

  file_source.update_file("/allowed_metrics.fst", FakeRemoteFileSource::make_fst(&[]));
  file_source.update_file(
    "/allowed_metric_patterns.csv",
    FakeRemoteFileSource::make_regex_csv(&[]),
  );

  helper
    .stats_helper()
    .wait_for_counter_eq(
      2,
      "pulse_proxy:pipeline:processor:elision:filter:poll:load_total",
      &labels! { "filter_name" => "exact_allowlist"},
    )
    .await;
  helper
    .stats_helper()
    .wait_for_counter_eq(
      2,
      "pulse_proxy:pipeline:processor:elision:filter:poll:load_total",
      &labels! { "filter_name" => "regex_allowlist"},
    )
    .await;

  write_all(
    &mut stream,
    &[
      "hello 1 2 source=server-0\n",
      "hello 1 3 source=server-0\n",
      "world 1 2 source=server-0\n",
      "world 1 3 source=server-0\n",
      "barbaz 1 2 source=server-0\n",
      "barbaz 1 3 source=server-0\n",
      "foo 100 3 source=server-0\n",
    ],
  )
  .await;

  assert_eq!(
    parse_carbon_metrics(&["foo 100 3 source=server-0\n",]),
    upstream.wait_for_metrics().await
  );

  assert_eq!(
    r"
filter name: exact_allowlist

filter name: regex_allowlist

filter name: blocklist
barbaz
hello
world
    "
    .trim(),
    make_admin_request(bind_resolver.local_tcp_addr("admin"), "/dump_poll_filter")
      .await
      .trim()
  );

  helper.shutdown().await;
}

fmt_reuse! {
BLOCKLISTING_LOCAL_FILES = r#"
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
        emit:
          consistent_every_period:
            periods: 2
        blocklist:
          allowed_metric_patterns:
            local:
              runtime_config:
                dir: "src/test/integration/blocklisting"
                file: "src/test/integration/blocklisting/allowlist.csv"
          blocked_metrics:
            local:
              runtime_config:
                dir: "src/test/integration/blocklisting"
                file: "src/test/integration/blocklisting/blocklist.fst"

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
async fn blocklisting_local_files() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:tcp"], &[]).await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let helper = Helper::new(
    &fmt!(
      BLOCKLISTING_LOCAL_FILES,
      fake_upstream = bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
  )
  .await;
  let mut stream = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp"))
    .await
    .unwrap();

  let current_time = 60;
  let lines = &[
    format!("foo.bar 1 {current_time} source=server-0\n"), // reported
    format!("foo.bar 1 {} source=server-0\n", current_time * 2), // elided
    format!("foo.bar 1 {} source=server-0\n", current_time * 3), // reported
    format!("foo.bar 1 {} source=server-0\n", current_time * 4), // elided
    format!("foo.bar 1 {} source=server-0\n", current_time * 5), // ...
    format!("foo.bar 1 {} source=server-0\n", current_time * 6),
    format!("foo.bar 1 {} source=server-0\n", current_time * 7),
    format!("foo.bar 1 {} source=server-0\n", current_time * 8),
    format!("foo.bar 1 {} source=server-0\n", current_time * 9),
    format!("foo.bar.baz 1 {current_time} source=server-0\n"), // allowlisted, reported
    format!("foo.bar.baz 1 {} source=server-0\n", current_time * 2),
    format!("foo.bar.baz 1 {} source=server-0\n", current_time * 3),
    format!("foo.bar.baz 1 {} source=server-0\n", current_time * 4),
    format!("hello.world 1 {current_time} source=server-0\n"), // not blocklisted, reported
    format!("hello.world 1 {} source=server-0\n", current_time * 2),
    format!("hello.world 1 {} source=server-0\n", current_time * 3),
    format!("hello.world 1 {} source=server-0\n", current_time * 4),
  ];

  write_all(
    &mut stream,
    &lines.iter().map(std::string::String::as_str).collect_vec(),
  )
  .await;

  let mut expected = vec![];
  expected.extend((0 ..= 8).step_by(2).map(|i| &lines[i]));
  expected.extend(&lines[9 ..]);

  assert_eq!(
    parse_carbon_metrics(&expected.iter().map(|s| s.as_str()).collect_vec()),
    upstream.wait_for_metrics().await
  );

  helper.shutdown().await;
}

fmt_reuse! {
BLOCKLISTING_ALL = r#"
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
          emit:
            consistent_every_period:
              periods: 240
              period_seconds: 15
          blocklist:
            allowed_metric_patterns:
              local:
                runtime_config:
                  dir: "src/test/integration/blocklisting"
                  file: "src/test/integration/blocklisting/allowlist.csv"
            block_all: true

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
async fn blocklisting_block_all() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:tcp"], &[]).await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let helper = Helper::new(
    &fmt!(
      BLOCKLISTING_ALL,
      fake_upstream = bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
  )
  .await;
  let mut stream = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp"))
    .await
    .unwrap();

  let current_time = datetime!(2023-10-01 00:00:00 UTC).unix_timestamp();
  let lines = (0 .. 480)
    .map(|i| format!("foo 1 {} source=server-0\n", current_time + i * 15))
    .collect_vec();

  write_all(
    &mut stream,
    &lines.iter().map(std::string::String::as_str).collect_vec(),
  )
  .await;

  assert_eq!(2, upstream.wait_for_metrics().await.len());

  helper.shutdown().await;
}
