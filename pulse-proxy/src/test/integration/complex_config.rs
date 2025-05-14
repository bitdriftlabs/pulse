// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::test::integration::{FakeHttpUpstream, Helper, HelperBindResolver};
use pretty_assertions::assert_eq;
use prom_remote_write::prom_remote_write_server_config::ParseConfig;
use prometheus::labels;
use pulse_common::k8s::pods_info::container::PodsInfo;
use pulse_common::k8s::test::make_pod_info;
use pulse_metrics::protos::metric::{DownstreamId, MetricSource, MetricType, MetricValue};
use pulse_metrics::test::{clean_timestamps, make_counter, make_gauge, make_metric_ex};
use pulse_protobuf::protos::pulse::config::inflow::v1::prom_remote_write;
use reusable_fmt::{fmt, fmt_reuse};
use std::collections::HashMap;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::watch;
use uuid::Uuid;
use vrl::btreemap;

fmt_reuse! {
COMPLEX_CONFIG = r#"
kubernetes:
  evaluate_services: false
  node_name:
    env_var: "K8S_NODE_NAME"

pipeline:
  inflows:
    statsd_tcp:
      routes: ["processor:regex_filter"]
      tcp:
        bind: "inflow:tcp"
        bind_k8s_pod_metadata_by_remote_ip: true
        protocol:
          statsd:
            lyft_tags: true
        advanced:
          idle_timeout: 300s

    statsd_udp:
      routes: ["processor:regex_filter"]
      udp:
        bind: "inflow:udp"
        bind_k8s_pod_metadata_by_remote_ip: true
        protocol:
          statsd:
            lyft_tags: true

  processors:
    regex_filter:
      routes: ["processor:cardinality"]
      regex:
        deny:
          - "spark\\.[^\\.]*\\.[0-9]*\\."

    cardinality:
      routes:
        - "processor:general_k8s_filter"
        - "processor:envoy_edge_filter"
        - "processor:instance_mutate"
      cardinality_limiter:
        per_pod_limit:
          default_size_limit: 90000
          vrl_program: |
            service = string!(%k8s.pod.annotations.service)
            if match_any(service, [
              r'^fare$',
              r'^fareinternal$',
              r'^faredriver$',
              r'^dispatch$',
              r'^dispatchglobalmatchingworkers$',
              r'^dispatchmatchingworkerssmall$',
              r'^ghostmatching$',
              r'^k8slogproxy$',
              r'^karma$',
              r'^matching$',
              r'^matchingsmall$',
              r'^matchingpipeline$',
              r'^matchingpipelinescheduler$',
              r'^offerings$',
              r'^pricingrealtime$',
              r'^modes$',
            ]) {{
              200000
            }} else if service == "ufo" {{
              180000
            }} else if service == "analyticsmetrics" {{
              10000001
            }} else {{
              90000
            }}

        buckets: 2
        rotate_after: 1800s

    #
    # Instance metrics
    #

    instance_mutate:
      routes: ["processor:instance_populate_cache"]
      mutate:
        vrl_program: |
          if !(match_any(.name, [
              r'pylogging',
              r'envoy\.cluster\.local_service',
              ]) || .tags._f == "i") {{
            abort
          }}

          # Verify expected annotations on entry.
          service = string!(%k8s.pod.annotations.service)
          service_name = string!(%k8s.pod.annotations.service_name)
          facet = to_string(%k8s.pod.annotations.facet)
          ec2_region_shortname = string!(%k8s.pod.annotations.ec2_region_shortname)
          ec2_az = get_env_var!("{az_env_var}")
          pod_name = string!(%k8s.pod.name)
          canary = to_string(%k8s.pod.annotations.canary)

          # These are parsed by WFP as "extra"
          .tags._x_instance = pod_name
          .tags._x_az = ec2_az
          .tags._x_region = ec2_region_shortname
          .tags._x_project = service_name
          .tags._x_facet = facet
          .tags._x_service = service
          if canary == "true" {{
            .tags._x_canary = "true"
          }}

    instance_populate_cache:
      routes: ["processor:instance_aggregation"]
      populate_cache: {{}}

    instance_aggregation:
      routes: ["outflow:wfp"]
      aggregation:
        quantile_timers:
          prefix: "timers."
          eps: 0.001
          quantiles: [0.5, 0.95, 0.99]
          extended:
            mean: true
            count: true
            upper: true
        counters:
          prefix: "counters."
          extended:
            sum: true
            rate: true
        gauges:
          prefix: "gauges."
          extended:
            sum: true
            mean: true
            min: true
            max: true
        flush_interval: 1s
        enable_last_aggregation_admin_endpoint: true

    #
    # Generic K8s
    #

    general_k8s_filter:
      routes:
        - "processor:general_k8s_central_mutate"
        - "processor:general_k8s_canary_mutate"
        - "processor:general_k8s_envoy_az_mutate"
      mutate:
        vrl_program: |
          if string!(%k8s.pod.annotations.service_name) == "envoyedge" {{
            abort
          }}

    general_k8s_central_mutate:
      routes: ["processor:central_populate_cache"]
      mutate:
        vrl_program: |
          # Verify expected annotations on entry.
          service = string!(%k8s.pod.annotations.service)
          service_name = string!(%k8s.pod.annotations.service_name)
          facet = to_string(%k8s.pod.annotations.facet)
          service_instance = string!(%k8s.pod.annotations.service_instance)
          ec2_region_shortname = string!(%k8s.pod.annotations.ec2_region_shortname)

          .name = join!([service, facet, "-", service_instance, "-", ec2_region_shortname, ".",
                        .name])
          .tags.facet = facet
          .tags.project = service_name
          .tags.envoyservice = service

    general_k8s_canary_mutate:
      routes: ["processor:central_populate_cache"]
      mutate:
        vrl_program: |
          facet = to_string(%k8s.pod.annotations.facet)
          if !starts_with(facet, "canary") {{
            abort
          }}

          # Verify expected annotations on entry.
          service = string!(%k8s.pod.annotations.service)
          service_name = string!(%k8s.pod.annotations.service_name)
          service_instance = string!(%k8s.pod.annotations.service_instance)
          ec2_region_shortname = string!(%k8s.pod.annotations.ec2_region_shortname)

          .name = join!([service, "-", service_instance, "-", ec2_region_shortname, ".",
                        .name, ".instance.",
                        service, "-", service_instance, "-", ec2_region_shortname, "-", facet])
          .tags.facet = facet
          .tags.project = service_name
          .tags.envoyservice = service

    general_k8s_envoy_az_mutate:
      routes: ["processor:central_populate_cache"]
      mutate:
        vrl_program: |
          if !match_any(.name, [
            r'envoy\.http\..*\.downstream_rq_5xx',
            r'envoy\.http\..*\.downstream_rq_completed',
            r'envoy\.http\..*\.downstream_rq_total',
            r'envoy\.cluster\..*\.upstream_rq_time',
            r'envoy\.cluster\..*\.upstream_rq_5xx',
            r'envoy\.cluster\..*\.outlier_detection\.ejections_enforced_total',
          ]) {{
            abort
          }}

          # Verify expected annotations on entry.
          service = string!(%k8s.pod.annotations.service)
          service_name = string!(%k8s.pod.annotations.service_name)
          facet = to_string(%k8s.pod.annotations.facet)
          service_instance = string!(%k8s.pod.annotations.service_instance)
          ec2_region_shortname = string!(%k8s.pod.annotations.ec2_region_shortname)
          ec2_az = get_env_var!("{az_env_var}")

          .name = join!([service, facet, "-", service_instance, "-", ec2_region_shortname, ".",
                        .name])
          .tags.facet = facet
          .tags.project = service_name
          .tags.envoyservice = service

          # default the envoyaz to "other" to prevent duplication
          # of metrics at aggregation layer created by absense of the tag.
          locality_zone_tag = "other"
          allowed_az = "us-east-1a"
          if service_instance == "staging" {{
            # in staging env, envoyminimesh test cluster is in 1a AZ
            # while the core-staging test cluster is in 1b AZ
            # since envoyminimesh does not have services deployed
            # we need to allow 2 AZs.
            allowed_az = "us-east-1a,us-east-1b"
          }}
          if contains(allowed_az, ec2_az) {{
            locality_zone_tag = ec2_az
          }}
          .tags.envoyaz = locality_zone_tag

    #
    # Envoy edge
    #

    envoy_edge_filter:
      routes:
        - "processor:envoy_edge_central_mutate"
        - "processor:envoy_edge_canary_mutate"
        - "processor:envoy_edge_envoy_az_mutate"
      mutate:
        vrl_program: |
          if string!(%k8s.pod.annotations.service_name) != "envoyedge" {{
            abort
          }}

    envoy_edge_central_mutate:
      routes: ["processor:central_populate_cache"]
      mutate:
        vrl_program: |
          # Verify expected annotations on entry.
          service = string!(%k8s.pod.annotations.service)
          facet = to_string(%k8s.pod.annotations.facet)
          service_instance = string!(%k8s.pod.annotations.service_instance)
          ec2_region_shortname = string!(%k8s.pod.annotations.ec2_region_shortname)

          .name = join!([service, facet, "-", service_instance, "-", ec2_region_shortname, ".",
                        .name])
          .tags.facet = facet

    envoy_edge_canary_mutate:
      routes: ["processor:central_populate_cache"]
      mutate:
        vrl_program: |
          # Verify expected annotations on entry.
          service = string!(%k8s.pod.annotations.service)
          facet = to_string(%k8s.pod.annotations.facet)
          service_instance = string!(%k8s.pod.annotations.service_instance)
          ec2_region_shortname = string!(%k8s.pod.annotations.ec2_region_shortname)
          pod_name = string!(%k8s.pod.name)

          .name = join!([service, "-", service_instance, "-", ec2_region_shortname, ".",
                        .name, ".instance.",
                        service, "-", service_instance, "-", ec2_region_shortname, "-", pod_name])
          .tags.facet = facet

    envoy_edge_envoy_az_mutate:
      routes: ["processor:central_populate_cache"]
      mutate:
        vrl_program: |
          if !match_any(.name, [
            r'envoy\.http\..*\.downstream_rq_5xx',
            r'envoy\.http\..*\.downstream_rq_completed',
            r'envoy\.http\..*\.downstream_rq_total',
          ]) {{
            abort
          }}

          # Verify expected annotations on entry.
          service = string!(%k8s.pod.annotations.service)
          facet = to_string(%k8s.pod.annotations.facet)
          service_instance = string!(%k8s.pod.annotations.service_instance)
          ec2_region_shortname = string!(%k8s.pod.annotations.ec2_region_shortname)
          ec2_az = get_env_var!("{az_env_var}")

          .name = join!([service, facet, "-", service_instance, "-", ec2_region_shortname, ".",
                        .name])

          # default the envoyaz to "other" to prevent duplication
          # of metrics at aggregation layer created by absense of the tag.
          locality_zone_tag = "other"
          allowed_az = "us-east-1a,us-east-1b,us-east-1c,us-east-1d,us-east-1e,us-east-1f"
          if service_instance == "staging" {{
            # in staging env, envoyminimesh test cluster is in 1a AZ
            # while the core-staging test cluster is in 1b AZ
            # since envoyminimesh does not have services deployed
            # we need to allow 2 AZs.
            allowed_az = "us-east-1a,us-east-1b"
          }}
          if contains(allowed_az, ec2_az) {{
            locality_zone_tag = ec2_az
          }}
          .tags.envoyaz = locality_zone_tag

    central_populate_cache:
      routes: ["processor:central_aggregation"]
      populate_cache: {{}}

    central_aggregation:
      routes: ["outflow:central"]
      aggregation:
        flush_interval: 1s
        enable_last_aggregation_admin_endpoint: true
        reservoir_timers:
          emit_as_bulk_timer: true

  outflows:
    wfp:
      prom_remote_write:
        send_to: "http://{fake_wfp}/api/v1/prom/write"
        request_timeout: 30s # Seems high but statsite default.
        batch_max_samples: 1000 # Default. Leaving here for tuning.
        queue_policy:
          queue_max_bytes: 1310720
        # WFP expects statsd style names
        convert_metric_name: false

    central:
      prom_remote_write:
        send_to: "http://{fake_central}/api/v1/prom/write"
        # WFP expects statsd style names
        convert_metric_name: false
    "#;
}

#[tokio::test]
async fn all() {
  let bind_resolver =
    HelperBindResolver::new(&["fake_wfp", "fake_central", "inflow:tcp"], &["inflow:udp"]).await;
  let mut fake_wfp = FakeHttpUpstream::new_prom(
    "fake_wfp",
    bind_resolver.clone(),
    ParseConfig {
      summary_as_timer: true,
      counter_as_delta: true,
      ..Default::default()
    },
  )
  .await;
  let mut fake_central = FakeHttpUpstream::new_prom(
    "fake_central",
    bind_resolver.clone(),
    ParseConfig {
      summary_as_timer: true,
      counter_as_delta: true,
      ..Default::default()
    },
  )
  .await;
  let mut pods_info = PodsInfo::default();
  pods_info.insert(
    make_pod_info(
      "default",
      "pod_a",
      &btreemap!(),
      btreemap!(
        "service" => "ghostmatching",
        "service_name" => "ghostmatchingname",
        "facet" => "main",
        "service_instance" => "production",
        "ec2_region_shortname" => "xyz"
      ),
      HashMap::default(),
      "127.0.0.1",
      vec![],
    ),
    true,
  );
  let (_pods_tx, pods_rx) = watch::channel(pods_info);
  let az_env_var = Uuid::new_v4().to_string();
  unsafe {
    std::env::set_var(&az_env_var, "us-east-1b");
  }
  let helper = Helper::new_with_k8s(
    &fmt!(
      COMPLEX_CONFIG,
      fake_wfp = bind_resolver.local_tcp_addr("fake_wfp"),
      fake_central = bind_resolver.local_tcp_addr("fake_central"),
      az_env_var = az_env_var,
    ),
    bind_resolver.clone(),
    Some(pods_rx),
  )
  .await;

  let mut stream = TcpStream::connect(bind_resolver.local_tcp_addr("inflow:tcp"))
    .await
    .unwrap();
  stream.write_all(b"spark.foo.123.bar:1|c\n").await.unwrap();
  helper
    .stats_helper()
    .wait_for_counter_eq(
      1,
      "pulse_proxy:pipeline:processor:regex_filter:drop",
      &labels! {},
    )
    .await;

  // Instance metrics
  stream
    .write_all(b"envoy.cluster.local_service_grpc.bar:1|c\n")
    .await
    .unwrap();
  stream.write_all(b"foo.___f=i:1|c\n").await.unwrap();
  // Non-instance metrics
  stream.write_all(b"blah.baz.__tag_a=a:1|g\n").await.unwrap();
  stream
    .write_all(b"blah.boop.__tag_b=b:1|ms\n")
    .await
    .unwrap();

  assert_eq!(
    vec![
      make_counter(
        "counters.envoy.cluster.local_service_grpc.bar.rate",
        &[
          ("_x_az", "us-east-1b"),
          ("_x_facet", "main"),
          ("_x_instance", "pod_a"),
          ("_x_project", "ghostmatchingname"),
          ("_x_region", "xyz"),
          ("_x_service", "ghostmatching")
        ],
        0,
        1.0
      ),
      make_counter(
        "counters.envoy.cluster.local_service_grpc.bar.sum",
        &[
          ("_x_az", "us-east-1b"),
          ("_x_facet", "main"),
          ("_x_instance", "pod_a"),
          ("_x_project", "ghostmatchingname"),
          ("_x_region", "xyz"),
          ("_x_service", "ghostmatching")
        ],
        0,
        1.0
      ),
      make_counter(
        "counters.foo.rate",
        &[
          ("_f", "i"),
          ("_x_az", "us-east-1b"),
          ("_x_facet", "main"),
          ("_x_instance", "pod_a"),
          ("_x_project", "ghostmatchingname"),
          ("_x_region", "xyz"),
          ("_x_service", "ghostmatching")
        ],
        0,
        1.0
      ),
      make_counter(
        "counters.foo.sum",
        &[
          ("_f", "i"),
          ("_x_az", "us-east-1b"),
          ("_x_facet", "main"),
          ("_x_instance", "pod_a"),
          ("_x_project", "ghostmatchingname"),
          ("_x_region", "xyz"),
          ("_x_service", "ghostmatching")
        ],
        0,
        1.0
      )
    ],
    clean_timestamps(fake_wfp.wait_for_metrics().await.1)
  );
  assert_eq!(
    vec![
      make_gauge(
        "ghostmatchingmain-production-xyz.blah.baz",
        &[
          ("envoyservice", "ghostmatching"),
          ("facet", "main"),
          ("project", "ghostmatchingname"),
          ("tag_a", "a"),
        ],
        0,
        1.0
      ),
      make_metric_ex(
        "ghostmatchingmain-production-xyz.blah.boop",
        &[
          ("envoyservice", "ghostmatching"),
          ("facet", "main"),
          ("project", "ghostmatchingname"),
          ("tag_b", "b"),
        ],
        0,
        Some(MetricType::BulkTimer),
        Some(1.0),
        MetricValue::BulkTimer(vec![1.0]),
        MetricSource::PromRemoteWrite,
        DownstreamId::LocalOrigin,
        None,
      ),
      make_counter(
        "ghostmatchingmain-production-xyz.envoy.cluster.local_service_grpc.bar",
        &[
          ("envoyservice", "ghostmatching"),
          ("facet", "main"),
          ("project", "ghostmatchingname")
        ],
        0,
        1.0
      ),
      make_counter(
        "ghostmatchingmain-production-xyz.foo",
        &[
          ("_f", "i"),
          ("envoyservice", "ghostmatching"),
          ("facet", "main"),
          ("project", "ghostmatchingname")
        ],
        0,
        1.0
      )
    ],
    clean_timestamps(fake_central.wait_for_metrics().await.1)
  );

  // Verify the changed type warning and tracker.
  stream
    .write_all(b"blah.baz:1|g\nblah.baz:1|c\n")
    .await
    .unwrap();
  tokio::time::sleep(std::time::Duration::from_secs(2)).await;
  helper
    .stats_helper()
    .wait_for_counter_eq(
      1,
      "pulse_proxy:pipeline:outflow:central:changed_type",
      &labels! {"family_name" => "ghostmatchingmain-production-xyz.blah.baz"},
    )
    .await;

  helper.shutdown().await;
}
