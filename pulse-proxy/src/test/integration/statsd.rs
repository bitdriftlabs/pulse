// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::test::integration::{FakeWireUpstream, Helper, HelperBindResolver};
use bd_time::TimeDurationExt;
use pretty_assertions::assert_eq;
use prometheus::labels;
use pulse_common::k8s::pods_info::container::PodsInfo;
use pulse_common::k8s::test::make_pod_info;
use pulse_metrics::test::{clean_timestamps, parse_carbon_metrics};
use reusable_fmt::{fmt, fmt_reuse};
use std::collections::HashMap;
use time::ext::NumericalDuration;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::watch;
use vrl::btreemap;

fmt_reuse! {
STATSD_UDP_CONFIG = r#"
  pipeline:
    inflows:
      udp:
        routes: ["outflow:tcp"]
        udp:
          bind: "inflow:udp"
          protocol:
            statsd:
              lyft_tags: true

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
async fn udp() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream"], &["inflow:udp"]).await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let helper = Helper::new(
    &fmt!(
      STATSD_UDP_CONFIG,
      bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
  )
  .await;
  let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
  socket
    .connect(bind_resolver.take_udp_addr("inflow:udp"))
    .await
    .unwrap();
  socket.send(b"foo.__bar=baz:1|c").await.unwrap();

  assert_eq!(
    clean_timestamps(parse_carbon_metrics(&["foo 1 bar=baz\n"])),
    clean_timestamps(upstream.wait_for_metrics().await)
  );

  socket
    .send(b"foo.__bar=baz:1|c\nhello.__world=blah:1|c")
    .await
    .unwrap();

  assert_eq!(
    clean_timestamps(parse_carbon_metrics(&[
      "foo 1 bar=baz\n",
      "hello 1 world=blah\n"
    ])),
    clean_timestamps(upstream.wait_for_metrics().await)
  );

  helper.shutdown().await;
}

fmt_reuse! {
STATSD_UDP_K8S_METADATA_CONFIG = r#"
    pipeline:
      inflows:
        udp:
          routes: ["processor:pod_mutate"]
          udp:
            bind: "inflow:udp"
            protocol:
              statsd: {{}}
            bind_k8s_pod_metadata_by_remote_ip: true

      processors:
        pod_mutate:
          routes: ["outflow:tcp"]
          mutate:
            vrl_program: |
              .tags.namespace = string!(%k8s.namespace)
              .tags.pod = string!(%k8s.pod.name)
              .tags.foo_label = string!(%k8s.pod.labels.foo)
              .tags.foo_annotation = string!(%k8s.pod.annotations.foo)

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
async fn udp_k8s_pod_metadata() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream"], &["inflow:udp"]).await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let mut pods_info = PodsInfo::default();
  pods_info.insert(make_pod_info(
    "default",
    "pod_a",
    &btreemap!("foo" => "bar"),
    btreemap!("foo" => "baz"),
    HashMap::default(),
    "127.0.0.1",
    vec![],
  ));
  let (_pods_tx, pods_rx) = watch::channel(pods_info);

  let helper = Helper::new_with_k8s(
    &fmt!(
      STATSD_UDP_K8S_METADATA_CONFIG,
      bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
    Some(pods_rx),
  )
  .await;
  let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
  socket
    .connect(bind_resolver.take_udp_addr("inflow:udp"))
    .await
    .unwrap();
  socket.send(b"foo:1|c\n").await.unwrap();

  assert_eq!(
    clean_timestamps(parse_carbon_metrics(&["foo 1 foo_annotation=baz \
                                             foo_label=bar namespace=default \
                                             pod=pod_a\n"])),
    clean_timestamps(upstream.wait_for_metrics().await)
  );

  helper.shutdown().await;
}

fmt_reuse! {
STATSD_UDP_PRE_BUFFER_CONFIG = r#"
      pipeline:
        inflows:
          udp:
            routes: ["processor:pod_mutate"]
            udp:
              bind: "inflow:udp"
              protocol:
                statsd: {{}}
              bind_k8s_pod_metadata_by_remote_ip: true
              pre_buffer:
                pre_buffer_window: 0.5s
                session_idle_timeout: 1s
                always_pre_buffer: true

        processors:
          pod_mutate:
            routes: ["outflow:tcp"]
            mutate:
              vrl_program: |
                .tags.namespace = string!(%k8s.namespace)
                .tags.pod = string!(%k8s.pod.name)
                .tags.foo_label = string!(%k8s.pod.labels.foo)
                .tags.foo_annotation = string!(%k8s.pod.annotations.foo)

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
async fn udp_k8s_pre_buffer() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream"], &["inflow:udp"]).await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let mut pods_info = PodsInfo::default();
  pods_info.insert(make_pod_info(
    "default",
    "pod_a",
    &btreemap!("foo" => "bar"),
    btreemap!("foo" => "baz"),
    HashMap::default(),
    "127.0.0.1",
    vec![],
  ));
  let (_pods_tx, pods_rx) = watch::channel(pods_info);

  let helper = Helper::new_with_k8s(
    &fmt!(
      STATSD_UDP_PRE_BUFFER_CONFIG,
      bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
    Some(pods_rx),
  )
  .await;
  let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
  socket
    .connect(bind_resolver.take_udp_addr("inflow:udp"))
    .await
    .unwrap();

  socket.send(b"foo:1|c\nfoo:1|c\n").await.unwrap();
  assert_eq!(
    clean_timestamps(parse_carbon_metrics(&["foo 2 foo_annotation=baz \
                                             foo_label=bar namespace=default \
                                             pod=pod_a\n"])),
    clean_timestamps(upstream.wait_for_metrics().await)
  );

  socket.send(b"bar:1|c\nbar:1|c\n").await.unwrap();
  assert_eq!(
    clean_timestamps(parse_carbon_metrics(&["bar 2 foo_annotation=baz \
                                             foo_label=bar namespace=default \
                                             pod=pod_a\n"])),
    clean_timestamps(upstream.wait_for_metrics().await)
  );

  // This is enough time for the session to idle timeout.
  // TODO(mattklein123) Add metrics or figure out some other way to better test this,
  2.seconds().sleep().await;
  socket.send(b"bar:1|c\n").await.unwrap();

  assert_eq!(
    clean_timestamps(parse_carbon_metrics(&["bar 1 foo_annotation=baz \
                                             foo_label=bar namespace=default \
                                             pod=pod_a\n"])),
    clean_timestamps(upstream.wait_for_metrics().await)
  );

  helper.shutdown().await;
}

fmt_reuse! {
STATSD_TCP_K8S_METADATA_CONFIG = r#"
      pipeline:
        inflows:
          tcp:
            routes: ["processor:pod_mutate"]
            tcp:
              bind: "inflow:tcp"
              protocol:
                statsd: {{}}
              bind_k8s_pod_metadata_by_remote_ip: true

        processors:
          pod_mutate:
            routes: ["outflow:tcp"]
            mutate:
              vrl_program: |
                .tags.namespace = string!(%k8s.namespace)
                .tags.pod = string!(%k8s.pod.name)
                .tags.foo_label = string!(%k8s.pod.labels.foo)
                .tags.foo_annotation = string!(%k8s.pod.annotations.foo)

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
async fn tcp_k8s_pod_metadata() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:tcp"], &[]).await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let mut pods_info = PodsInfo::default();
  pods_info.insert(make_pod_info(
    "default",
    "pod_a",
    &btreemap!("foo" => "bar"),
    btreemap!("foo" => "baz"),
    HashMap::default(),
    "127.0.0.1",
    vec![],
  ));
  let (_pods_tx, pods_rx) = watch::channel(pods_info);

  let helper = Helper::new_with_k8s(
    &fmt!(
      STATSD_TCP_K8S_METADATA_CONFIG,
      bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
    Some(pods_rx),
  )
  .await;
  let mut stream = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp"))
    .await
    .unwrap();
  stream.write_all(b"foo:1|c\n").await.unwrap();

  assert_eq!(
    clean_timestamps(parse_carbon_metrics(&["foo 1 foo_annotation=baz \
                                             foo_label=bar namespace=default \
                                             pod=pod_a\n"])),
    clean_timestamps(upstream.wait_for_metrics().await)
  );

  helper.shutdown().await;
}

#[tokio::test]
async fn tcp_k8s_pod_metadata_missing_pod() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:tcp"], &[]).await;
  let pods_info = PodsInfo::default();
  let (_pods_tx, pods_rx) = watch::channel(pods_info);

  let helper = Helper::new_with_k8s(
    &fmt!(
      STATSD_TCP_K8S_METADATA_CONFIG,
      bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
    Some(pods_rx),
  )
  .await;
  let mut stream = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp"))
    .await
    .unwrap();
  stream.write_all(b"foo:1|c\n").await.unwrap();

  helper
    .stats_helper()
    .wait_for_counter_eq(
      1,
      "pulse_proxy:pipeline:inflow:tcp:no_k8s_pod_metadata",
      &labels! {},
    )
    .await;

  helper.shutdown().await;
}

fmt_reuse! {
STATSD_TCP_K8S_METADATA_PREBUFFER_CONFIG = r#"
        pipeline:
          inflows:
            tcp:
              routes: ["processor:pod_mutate"]
              tcp:
                bind: "inflow:tcp"
                protocol:
                  statsd: {{}}
                bind_k8s_pod_metadata_by_remote_ip: true
                pre_buffer_window: 1s

          processors:
            pod_mutate:
              routes: ["outflow:tcp"]
              mutate:
                vrl_program: |
                  .tags.namespace = string!(%k8s.namespace)
                  .tags.pod = string!(%k8s.pod.name)
                  .tags.foo_label = string!(%k8s.pod.labels.foo)
                  .tags.foo_annotation = string!(%k8s.pod.annotations.foo)

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
async fn tcp_k8s_prebuffer() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:tcp"], &[]).await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let mut pods_info = PodsInfo::default();
  pods_info.insert(make_pod_info(
    "default",
    "pod_a",
    &btreemap!("foo" => "bar"),
    btreemap!("foo" => "baz"),
    HashMap::default(),
    "127.0.0.1",
    vec![],
  ));
  let (_pods_tx, pods_rx) = watch::channel(pods_info);

  let helper = Helper::new_with_k8s(
    &fmt!(
      STATSD_TCP_K8S_METADATA_PREBUFFER_CONFIG,
      bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
    Some(pods_rx),
  )
  .await;
  let mut stream = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp"))
    .await
    .unwrap();
  // Demonstrate aggregation.
  stream
    .write_all(b"foo:1|c\nbar:2|g\nbaz:3|ms\nfoo:1|c\nbar:3|g\nbaz:4|ms\n")
    .await
    .unwrap();

  // Need to sort due to aggregation hash table.
  let mut metrics = upstream.wait_for_metrics().await;
  metrics.sort_by(|lhs, rhs| {
    lhs
      .metric()
      .get_id()
      .name()
      .cmp(rhs.metric().get_id().name())
  });
  assert_eq!(
    clean_timestamps(parse_carbon_metrics(&[
      "bar 3 foo_annotation=baz foo_label=bar namespace=default pod=pod_a\n",
      "baz 3 foo_annotation=baz foo_label=bar namespace=default pod=pod_a\n",
      "baz 4 foo_annotation=baz foo_label=bar namespace=default pod=pod_a\n",
      "foo 2 foo_annotation=baz foo_label=bar namespace=default pod=pod_a\n",
    ])),
    clean_timestamps(metrics)
  );

  // New metrics should come right away.
  stream
    .write_all(b"foo:1|c\nbar:2|g\nbaz:3|ms\nfoo:1|c\nbar:3|g\nbaz:4|ms\n")
    .await
    .unwrap();
  assert_eq!(
    clean_timestamps(parse_carbon_metrics(&[
      "foo 1 foo_annotation=baz foo_label=bar namespace=default pod=pod_a\n",
      "bar 2 foo_annotation=baz foo_label=bar namespace=default pod=pod_a\n",
      "baz 3 foo_annotation=baz foo_label=bar namespace=default pod=pod_a\n",
      "foo 1 foo_annotation=baz foo_label=bar namespace=default pod=pod_a\n",
      "bar 3 foo_annotation=baz foo_label=bar namespace=default pod=pod_a\n",
      "baz 4 foo_annotation=baz foo_label=bar namespace=default pod=pod_a\n",
    ])),
    clean_timestamps(upstream.wait_for_num_metrics(6).await)
  );

  helper.shutdown().await;
}

#[tokio::test]
async fn tcp_k8s_prebuffer_early_shutdown() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:tcp"], &[]).await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let mut pods_info = PodsInfo::default();
  pods_info.insert(make_pod_info(
    "default",
    "pod_a",
    &btreemap!("foo" => "bar"),
    btreemap!("foo" => "baz"),
    HashMap::default(),
    "127.0.0.1",
    vec![],
  ));
  let (_pods_tx, pods_rx) = watch::channel(pods_info);

  let helper = Helper::new_with_k8s(
    &fmt!(
      STATSD_TCP_K8S_METADATA_PREBUFFER_CONFIG,
      bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
    Some(pods_rx),
  )
  .await;
  let mut stream = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp"))
    .await
    .unwrap();
  // Demonstrate aggregation.
  stream
    .write_all(b"foo:1|c\nbar:2|g\nbaz:3|ms\nfoo:1|c\nbar:3|g\nbaz:4|ms\n")
    .await
    .unwrap();
  drop(stream);

  // Need to sort due to aggregation hash table.
  let mut metrics = upstream.wait_for_metrics().await;
  metrics.sort_by(|lhs, rhs| {
    lhs
      .metric()
      .get_id()
      .name()
      .cmp(rhs.metric().get_id().name())
  });
  assert_eq!(
    clean_timestamps(parse_carbon_metrics(&[
      "bar 3 foo_annotation=baz foo_label=bar namespace=default pod=pod_a\n",
      "baz 3 foo_annotation=baz foo_label=bar namespace=default pod=pod_a\n",
      "baz 4 foo_annotation=baz foo_label=bar namespace=default pod=pod_a\n",
      "foo 2 foo_annotation=baz foo_label=bar namespace=default pod=pod_a\n",
    ])),
    clean_timestamps(metrics)
  );

  helper.shutdown().await;
}

fmt_reuse! {
STATSD_TCP_K8S_METADATA_ALWAYS_PREBUFFER_CONFIG = r#"
          pipeline:
            inflows:
              tcp:
                routes: ["processor:pod_mutate"]
                tcp:
                  bind: "inflow:tcp"
                  protocol:
                    statsd: {{}}
                  bind_k8s_pod_metadata_by_remote_ip: true
                  pre_buffer_window: 1s
                  always_pre_buffer: true

            processors:
              pod_mutate:
                routes: ["outflow:tcp"]
                mutate:
                  vrl_program: |
                    .tags.namespace = string!(%k8s.namespace)
                    .tags.pod = string!(%k8s.pod.name)
                    .tags.foo_label = string!(%k8s.pod.labels.foo)
                    .tags.foo_annotation = string!(%k8s.pod.annotations.foo)

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
async fn tcp_k8s_always_prebuffer() {
  let bind_resolver = HelperBindResolver::new(&["fake_upstream", "inflow:tcp"], &[]).await;
  let mut upstream = FakeWireUpstream::new("fake_upstream", bind_resolver.clone()).await;
  let mut pods_info = PodsInfo::default();
  pods_info.insert(make_pod_info(
    "default",
    "pod_a",
    &btreemap!("foo" => "bar"),
    btreemap!("foo" => "baz"),
    HashMap::default(),
    "127.0.0.1",
    vec![],
  ));
  let (_pods_tx, pods_rx) = watch::channel(pods_info);

  let helper = Helper::new_with_k8s(
    &fmt!(
      STATSD_TCP_K8S_METADATA_ALWAYS_PREBUFFER_CONFIG,
      bind_resolver.local_tcp_addr("fake_upstream")
    ),
    bind_resolver.clone(),
    Some(pods_rx),
  )
  .await;
  let mut stream = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp"))
    .await
    .unwrap();
  // Demonstrate aggregation.
  stream
    .write_all(b"foo:1|c\nbar:2|g\nbaz:3|ms\nfoo:1|c\nbar:3|g\nbaz:4|ms\n")
    .await
    .unwrap();

  // Need to sort due to aggregation hash table.
  let mut metrics = upstream.wait_for_metrics().await;
  metrics.sort_by(|lhs, rhs| {
    lhs
      .metric()
      .get_id()
      .name()
      .cmp(rhs.metric().get_id().name())
  });
  assert_eq!(
    clean_timestamps(parse_carbon_metrics(&[
      "bar 3 foo_annotation=baz foo_label=bar namespace=default pod=pod_a\n",
      "baz 3 foo_annotation=baz foo_label=bar namespace=default pod=pod_a\n",
      "baz 4 foo_annotation=baz foo_label=bar namespace=default pod=pod_a\n",
      "foo 2 foo_annotation=baz foo_label=bar namespace=default pod=pod_a\n",
    ])),
    clean_timestamps(metrics)
  );

  // We still buffer.
  stream
    .write_all(b"foo:1|c\nbar:2|g\nbaz:3|ms\nfoo:1|c\nbar:3|g\nbaz:4|ms\n")
    .await
    .unwrap();
  let mut metrics = upstream.wait_for_metrics().await;
  metrics.sort_by(|lhs, rhs| {
    lhs
      .metric()
      .get_id()
      .name()
      .cmp(rhs.metric().get_id().name())
  });
  assert_eq!(
    clean_timestamps(parse_carbon_metrics(&[
      "bar 3 foo_annotation=baz foo_label=bar namespace=default pod=pod_a\n",
      "baz 3 foo_annotation=baz foo_label=bar namespace=default pod=pod_a\n",
      "baz 4 foo_annotation=baz foo_label=bar namespace=default pod=pod_a\n",
      "foo 2 foo_annotation=baz foo_label=bar namespace=default pod=pod_a\n",
    ])),
    clean_timestamps(metrics)
  );

  helper.shutdown().await;
}
