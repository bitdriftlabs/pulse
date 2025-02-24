// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::*;
use crate::build_stats_provider;
use config::bootstrap::v1::bootstrap::KubernetesBootstrapConfig;
use config::inflow::v1::inflow::inflow_config::Config_type as InflowConfigType;
use config::outflow::v1::outflow::outflow_config::Config_type as OutflowConfigType;
use pulse_common::bind_resolver::{BindResolver, BoundTcpSocket, make_reuse_port_tcp_socket};
use pulse_metrics::admin::test::MockAdmin;
use pulse_metrics::pipeline::{MetricPipeline, RealItemFactory};
use pulse_protobuf::protos::pulse::config;
use pulse_protobuf::protos::pulse::config::bootstrap::v1::bootstrap::{Config, PipelineConfig};
use std::io::Write;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::net::UdpSocket;

struct FakeBindResolver {}

#[async_trait::async_trait]
impl BindResolver for FakeBindResolver {
  async fn resolve_tcp(&self, _name: &str) -> anyhow::Result<BoundTcpSocket> {
    make_reuse_port_tcp_socket("localhost:0").await
  }

  async fn resolve_udp(&self, _name: &str) -> anyhow::Result<UdpSocket> {
    unimplemented!()
  }
}

async fn load_and_check_validity(path: &str) -> anyhow::Result<Config> {
  let config = load_from_file(path)?;
  let stats_provider = build_stats_provider(&config)?;
  MetricPipeline::new_from_config(
    Arc::new(RealItemFactory {}),
    stats_provider.collector().scope("test"),
    KubernetesBootstrapConfig::default(),
    Arc::new(|| unreachable!()),
    config.pipeline().clone(),
    Arc::default(),
    Arc::new(MockAdmin::default()),
    Arc::new(FakeBindResolver {}),
  )
  .await?;
  Ok(config)
}

// Do basic sanity testing of anything in the examples directory.
// TODO(mattklein123): Actually load the full example when split from bootstrap and pipeline into
// a single bootstrap and verify it.
#[tokio::test]
async fn verify_examples() {
  const BOOTSTRAP_EXAMPLES: &[&str] = &["bootstrap_agg.yaml", "bootstrap_ds.yaml"];
  const PIPELINE_EXAMPLES: &[&str] = &["config_agg.yaml", "config_ds.yaml"];

  let mut num_examples = 0;
  for example in std::fs::read_dir(env!("CARGO_MANIFEST_DIR").to_string() + "/../examples").unwrap()
  {
    let example = example.unwrap();
    if !example.file_type().unwrap().is_file() || !example.path().extension().unwrap().eq("yaml") {
      continue;
    }
    num_examples += 1;

    if BOOTSTRAP_EXAMPLES
      .iter()
      .any(|i| *i == example.file_name().to_str().unwrap())
    {
      let _config: Config = load_from_file(example.path().to_str().unwrap()).unwrap();
    } else if PIPELINE_EXAMPLES
      .iter()
      .any(|i| *i == example.file_name().to_str().unwrap())
    {
      let _pipeline: PipelineConfig = load_from_file(example.path().to_str().unwrap()).unwrap();
    } else {
      panic!("Unknown example file: {:?}", example.file_name());
    }
  }

  assert_eq!(
    num_examples,
    BOOTSTRAP_EXAMPLES.len() + PIPELINE_EXAMPLES.len()
  );
}

#[tokio::test]
async fn load_example_bad_config() {
  let config = r#"
admin:
  bind: "localhost:9999"

pipeline:
  inflows:
    tcp:
      routes: ["processor:elision"]
      tcp:
        bind: "[::]:3199"
        protocol:
          carbon: {}

  processors:
    elision:
      routes: ["outflow:tcp"]
      elision:
        emit: { ratio: 0.0 }

  outflows:
    tcp:
      tcp:
        common:
          send_to: "localhost:3198"
          protocol:
            carbon: {}
"#;
  let mut tf = NamedTempFile::new().unwrap();
  tf.write_all(config.as_bytes()).unwrap();

  assert_eq!(
    "elision ratio must be > 0.0 and <= 1.0",
    load_and_check_validity(tf.path().to_str().unwrap())
      .await
      .unwrap_err()
      .to_string()
  );
}

#[tokio::test]
async fn load_example_bad_config_with_emit_interval() {
  let config = r#"
admin:
  bind: "localhost:9999"

pipeline:
  inflows:
    tcp:
      routes: ["processor:elision"]
      tcp:
        bind: "[::]:3199"
        protocol:
          carbon: {}

  processors:
    elision:
      routes: ["outflow:tcp"]
      elision:
        emit: { interval: -10s }

  outflows:
    tcp:
      tcp:
        common:
          send_to: "localhost:3198"
          protocol:
            carbon: {}
"#;

  let mut tf = NamedTempFile::new().unwrap();
  tf.write_all(config.as_bytes()).unwrap();
  assert_eq!(
    "A proto validation error occurred: negative proto duration not supported",
    load_and_check_validity(tf.path().to_str().unwrap())
      .await
      .unwrap_err()
      .to_string()
  );
}

#[tokio::test]
async fn load_example_config() {
  let config = r#"
admin:
  bind: "localhost:9999"

meta_stats:
  meta_protocol:
    - wire:
        common:
          send_to: "localhost:3198"
          protocol:
            statsd: {}

pipeline:
  inflows:
    tcp:
      routes: ["processor:elision"]
      tcp:
        bind: "[::]:3199"
        protocol:
          statsd: {}

    prom_remote_write:
      routes: ["processor:elision"]
      prom_remote_write:
        bind: "[::]:3200"

  processors:
    elision:
      routes: ["outflow:tcp"]
      elision:
        emit: { ratio: 0.1 }

  outflows:
    tcp:
      tcp:
        common:
          send_to: "localhost:3198"
          protocol:
            statsd: {}
  "#;

  let mut tf = NamedTempFile::new().unwrap();
  tf.write_all(config.as_bytes()).unwrap();
  load_and_check_validity(tf.path().to_str().unwrap())
    .await
    .unwrap();
}

#[tokio::test]
async fn load_example_config_with_emit_interval() {
  let config = r#"
admin:
  bind: "localhost:9999"

pipeline:
  inflows:
    tcp:
      routes: ["processor:elision"]
      tcp:
        bind: "[::]:3199"
        protocol:
          statsd: {}

  processors:
    elision:
      routes: ["outflow:tcp"]
      elision:
        emit: { interval: 600s }

  outflows:
    tcp:
      tcp:
        common:
          send_to: "localhost:3198"
          protocol:
            statsd: {}
"#;

  let mut tf = NamedTempFile::new().unwrap();
  tf.write_all(config.as_bytes()).unwrap();
  load_and_check_validity(tf.path().to_str().unwrap())
    .await
    .unwrap();
}

#[tokio::test]
async fn load_regex_overrides_config() {
  let config = r#"
admin:
  bind: "localhost:9999"

pipeline:
  inflows:
    tcp:
      routes: ["processor:elision"]
      tcp:
        bind: "[::]:3199"
        protocol:
          statsd: {}

  processors:
    elision:
      routes: ["outflow:tcp"]
      elision:
        emit: { ratio: 0.1 }
        regex_overrides:
          - regex: "^node_.*"
            emit: { ratio: 0.2 }
          - regex: "^snmp._*"
            emit: { interval: 600s }

  outflows:
    tcp:
      tcp:
        common:
          send_to: "localhost:3198"
          protocol:
            statsd: {}
"#;

  let mut tf = NamedTempFile::new().unwrap();
  tf.write_all(config.as_bytes()).unwrap();
  load_and_check_validity(tf.path().to_str().unwrap())
    .await
    .unwrap();
}

#[tokio::test]
async fn load_example_send_to_internode_config() {
  let config = r#"
admin:
  bind: "localhost:9999"

pipeline:
  inflows:
    tcp:
      routes: ["processor:internode"]
      tcp:
        bind: "[::]:3199"
        protocol:
          statsd: {}

  processors:
    internode:
      routes: ["processor:elision"]
      alt_routes: ["outflow:tcp"]
      internode:
        listen: "[::]:3197"
        this_node_id: "node-1"
        nodes: [{node_id: "node-2", address: "[::]:4197"}]

    elision:
      routes: ["outflow:tcp"]
      elision:
        emit: { ratio: 0.1 }

  outflows:
    tcp:
      tcp:
        common:
          send_to: "localhost:3198"
          protocol:
            statsd: {}
"#;

  let mut tf = NamedTempFile::new().unwrap();
  tf.write_all(config.as_bytes()).unwrap();
  load_and_check_validity(tf.path().to_str().unwrap())
    .await
    .unwrap();
}

#[tokio::test]
async fn load_example_advanced_config() {
  let config = r#"
admin:
  bind: "localhost:9999"

pipeline:
  inflows:
    tcp:
      routes: ["processor:internode"]
      tcp:
        bind: "[::]:3199"
        protocol:
          statsd: {}
        advanced:
          idle_timeout: 99s
          buffer_size: 16384

  processors:
    internode:
      routes: ["processor:elision"]
      alt_routes: ["outflow:tcp"]
      internode:
        listen: "[::]:3197"
        this_node_id: "node-1"
        nodes:
          - address: "10.0.0.1:1234"
            node_id: "node-2"

    elision:
      routes: ["outflow:tcp"]
      elision:
        emit: { ratio: 0.1 }

  outflows:
    tcp:
      tcp:
        common:
          send_to: "localhost:3198"
          protocol:
            statsd: {}
  "#;

  let mut tf = NamedTempFile::new().unwrap();
  tf.write_all(config.as_bytes()).unwrap();
  let config = load_and_check_validity(tf.path().to_str().unwrap())
    .await
    .unwrap();

  let Some(InflowConfigType::Tcp(inflow)) = &config.pipeline().inflows["tcp"].config_type else {
    unreachable!()
  };

  assert_eq!(inflow.advanced.idle_timeout.as_ref().unwrap().seconds, 99);
  assert_eq!(inflow.advanced.buffer_size.unwrap(), 16384);

  let Some(OutflowConfigType::Tcp(outflow)) = &config.pipeline().outflows["tcp"].config_type else {
    unreachable!()
  };

  assert_eq!(outflow.common.write_timeout.as_ref(), None);
}

#[tokio::test]
async fn load_bad_internode_config() {
  let config = r#"
admin:
  bind: "localhost:9999"

pipeline:
  inflows:
    tcp:
      routes: ["processor:internode"]
      tcp:
        bind: "[::]:3199"
        protocol:
          carbon: {}
        advanced:
          idle_timeout: 99s
          buffer_size: 16384

  processors:
    internode:
      routes: ["processor:elision"]
      alt_routes: ["processor:elision"]
      internode:
        listen: "[::]:3200"
        total_nodes: 2
        this_node_id: "node-1"
        nodes:
          - {node_id: "node-2", address: "10.1.1.6:1234"}
          - {node_id: "node-3", address: "10.1.1.6:1234"}

    elision:
      routes: ["outflow:tcp"]
      elision:
        emit: { ratio: 0.1 }

  outflows:
    tcp:
      tcp:
        common:
          send_to: "localhost:3198"
          protocol:
            carbon: {}
"#;

  let mut tf = NamedTempFile::new().unwrap();
  tf.write_all(config.as_bytes()).unwrap();
  assert_eq!(
    "total_nodes count invalid (expected 3, found 2, including self). Did you include this node \
     in the count?",
    load_and_check_validity(tf.path().to_str().unwrap())
      .await
      .unwrap_err()
      .to_string()
  );
}

#[tokio::test]
async fn load_bad_regex_overrides_config() {
  let config = r#"
admin:
  bind: "localhost:9999"

pipeline:
  inflows:
    tcp:
      routes: ["processor:elision"]
      tcp:
        bind: "[::]:3199"
        protocol:
          carbon: {}

  processors:
    elision:
      routes: ["outflow:tcp"]
      elision:
        emit: { ratio: 0.1 }
        regex_overrides:
          - regex: "^node_.*"
            emit: { ratio: 1.2 }
          - regex: "^snmp._*"
            emit: { interval: 600s }

  outflows:
    tcp:
      tcp:
        common:
          send_to: "localhost:3198"
          protocol:
            carbon: {}
"#;

  let mut tf = NamedTempFile::new().unwrap();
  tf.write_all(config.as_bytes()).unwrap();
  assert_eq!(
    "elision ratio must be > 0.0 and <= 1.0",
    load_and_check_validity(tf.path().to_str().unwrap())
      .await
      .unwrap_err()
      .to_string()
  );
}

#[tokio::test]
async fn load_bad_regex_overrides_config_2() {
  let config = r#"
admin:
  bind: "localhost:9999"

pipeline:
  inflows:
    tcp:
      routes: ["processor:elision"]
      tcp:
        bind: "[::]:3199"
        protocol:
          carbon: {}

  processors:
    elision:
      routes: ["outflow:tcp"]
      elision:
        emit: { ratio: 0.1 }
        regex_overrides:
          - regex: "^node_.*"
            emit: { ratio: -0.2 }
          - regex: "^snmp._*"
            emit: { interval: 600s }

  outflows:
    tcp:
      tcp:
        common:
          send_to: "localhost:3198"
          protocol:
            carbon: {}
"#;

  let mut tf = NamedTempFile::new().unwrap();
  tf.write_all(config.as_bytes()).unwrap();
  assert_eq!(
    "elision ratio must be > 0.0 and <= 1.0",
    load_and_check_validity(tf.path().to_str().unwrap())
      .await
      .unwrap_err()
      .to_string()
  );
}

#[tokio::test]
async fn load_bad_regex_overrides_config_3() {
  let config = r#"
admin:
  bind: "localhost:9999"

pipeline:
  inflows:
    tcp:
      routes: ["processor:elision"]
      tcp:
        bind: "[::]:3199"
        protocol:
          statsd: {}

  processors:
    elision:
      routes: ["outflow:tcp"]
      elision:
        emit: { ratio: 0.1 }
        regex_overrides:
          - regex: "^node_.*"
            emit: { interval: -1s }
          - regex: "^snmp._*"
            emit: { ratio: 0.5 }

  outflows:
    tcp:
      tcp:
        common:
          send_to: "localhost:3198"
          protocol:
            statsd: {}
  "#;

  let mut tf = NamedTempFile::new().unwrap();
  tf.write_all(config.as_bytes()).unwrap();
  assert_eq!(
    "A proto validation error occurred: negative proto duration not supported",
    load_and_check_validity(tf.path().to_str().unwrap())
      .await
      .unwrap_err()
      .to_string()
  );
}

#[tokio::test]
async fn load_bad_protocol_config() {
  let config = r#"
admin:
  bind: "localhost:9999"

pipeline:
  inflows:
    tcp:
      routes: ["processor:elision"]
      tcp:
        bind: "[::]:3199"
        protocol:
          foo: {}
"#;

  let mut tf = NamedTempFile::new().unwrap();
  tf.write_all(config.as_bytes()).unwrap();
  assert_eq!(
    "Unknown field name: `foo` at 14:13",
    load_and_check_validity(tf.path().to_str().unwrap())
      .await
      .unwrap_err()
      .to_string()
  );
}

#[tokio::test]
async fn load_bad_stats_tags() {
  let config = r#"
admin:
  bind: "localhost:9999"

meta_stats:
  meta_protocol:
    - wire:
        common:
          send_to: "localhost:1234"
          protocol:
            statsd: {}
  meta_tag:
    - { key: "key" }

pipeline:
  inflows:
    tcp:
      routes: ["outflow:tcp"]
      tcp:
        bind: "[::]:3199"
        protocol:
          statsd: {}

  outflows:
    tcp:
      tcp:
        common:
          send_to: "localhost:3198"
          protocol:
            statsd: {}
  "#;

  let mut tf = NamedTempFile::new().unwrap();
  tf.write_all(config.as_bytes()).unwrap();
  assert_eq!(
    "A proto validation error occurred: field 'pulse.config.bootstrap.v1.MetaStats.MetaTag.value' \
     in message 'pulse.config.bootstrap.v1.MetaStats.MetaTag' requires string length >= 1",
    load_and_check_validity(tf.path().to_str().unwrap())
      .await
      .unwrap_err()
      .to_string()
  );

  let config = r#"
admin:
  bind: "localhost:9999"

meta_stats:
  meta_protocol:
    - wire:
        common:
          send_to: "localhost:1234"
          protocol:
            statsd: {}
  meta_tag:
    - { key: "source", value: "foo" }

pipeline:
  inflows:
    tcp:
      routes: ["outflow:tcp"]
      tcp:
        bind: "[::]:3199"
        protocol:
          statsd: {}

  outflows:
    tcp:
      tcp:
        common:
          send_to: "localhost:3198"
          protocol:
            statsd: {}
  "#;

  let mut tf = NamedTempFile::new().unwrap();
  tf.write_all(config.as_bytes()).unwrap();
  assert_eq!(
    "meta tag key cannot be \"source\"",
    load_and_check_validity(tf.path().to_str().unwrap())
      .await
      .unwrap_err()
      .to_string()
  );

  let config = r#"
admin:
  bind: "localhost:9999"

meta_stats:
  meta_protocol:
    - wire:
        common:
          send_to: "localhost:1234"
          protocol:
            statsd: {}
  meta_tag:
    - { key: "foo", value: "this=server" }

pipeline:
  inflows:
    tcp:
      routes: ["outflow:tcp"]
      tcp:
        bind: "[::]:3199"
        protocol:
          statsd: {}

  outflows:
    tcp:
      tcp:
        common:
          send_to: "localhost:3198"
          protocol:
            statsd: {}
  "#;

  let mut tf = NamedTempFile::new().unwrap();
  tf.write_all(config.as_bytes()).unwrap();
  assert_eq!(
    "meta tag keys and values cannot contain spaces, equals sign, or quotes",
    load_and_check_validity(tf.path().to_str().unwrap())
      .await
      .unwrap_err()
      .to_string()
  );
}

#[tokio::test]
async fn load_example_pipeline_config() {
  let config = r#"
admin:
  bind: "localhost:9999"

pipeline:
  inflows:
    carbon_tcp_server:
      routes: ["processor:elision"]
      tcp:
        bind: "[::]:3200"
        protocol:
          carbon: {}
        advanced:
          idle_timeout: "600s"
          buffer_size: 10000

    prom_remote_write_server:
      routes: ["processor:elision"]
      prom_remote_write:
        bind: "[::]:3201"

  processors:
    elision:
      routes: ["outflow:wavefront_proxy"]
      elision:
        emit: { ratio: 0.1 }
        analyze_mode: false

  outflows:
    wavefront_proxy:
      tcp:
        common:
          send_to: "1.1.1.1:3200"
          protocol:
            carbon: {}
"#;
  let mut tf = NamedTempFile::new().unwrap();
  tf.write_all(config.as_bytes()).unwrap();
  load_and_check_validity(tf.path().to_str().unwrap())
    .await
    .unwrap();
}

#[tokio::test]
async fn load_internode_pipeline_config() {
  let config = r#"
admin:
  bind: "localhost:9999"

pipeline:
  inflows:
    carbon_tcp_server:
      routes: ["processor:internode"]
      tcp:
        bind: "[::]:3200"
        protocol:
          carbon: {}
        advanced:
          idle_timeout: "600s"
          buffer_size: 10000

    prom_remote_write_server:
      routes: ["processor:internode"]
      prom_remote_write:
        bind: "[::]:3201"

  processors:
    internode:
      routes: ["processor:elision"]
      alt_routes: ["outflow:wavefront_proxy"]
      internode:
        listen: "[::]:9801"
        total_nodes: 2
        this_node_id: "node-1"
        nodes:
          - { node_id: "node-2", address: "localhost:9802" }

    elision:
      routes: ["outflow:wavefront_proxy"]
      elision:
        emit: { ratio: 0.1 }
        analyze_mode: false

  outflows:
    wavefront_proxy:
      tcp:
        common:
          send_to: "1.1.1.1:3200"
          protocol:
            carbon: {}
"#;
  let mut tf = NamedTempFile::new().unwrap();
  tf.write_all(config.as_bytes()).unwrap();
  load_and_check_validity(tf.path().to_str().unwrap())
    .await
    .unwrap();
}

#[tokio::test]
async fn simple_config_to_pipeline_config_saas() {
  let advanced_config = r#"
admin:
  bind: "localhost:9999"

pipeline:
  inflows:
    tcp:
      routes: ["processor:populate_cache"]
      tcp:
        bind: "[::]:3199"
        protocol:
          statsd: {}
        advanced:
          idle_timeout: "99s"
          buffer_size: 16384

    prom_remote_write:
      routes: ["processor:populate_cache"]
      prom_remote_write:
        bind: "[::]:3200"

  processors:
    populate_cache:
      routes: ["processor:elision", "processor:saas_sampler"]
      populate_cache: {}

    elision:
      routes: ["outflow:tcp"]
      elision:
        emit: { ratio: 0.1 }
        analyze_mode: true

    saas_sampler:
      routes: ["outflow:saas"]
      sampler:
        emit_interval: 3600s

  outflows:
    tcp:
      tcp:
        common:
          send_to: "localhost:3198"
          protocol:
            statsd: {}
          write_timeout: "30s"

    saas:
      prom_remote_write:
        send_to: "foo.com/v1/mme/ingest"
        metadata_only: true
        auth:
          bearer_token: { token: "1234" }
"#;
  let mut tf = NamedTempFile::new().unwrap();
  tf.write_all(advanced_config.as_bytes()).unwrap();
  load_and_check_validity(tf.path().to_str().unwrap())
    .await
    .unwrap();
}

#[tokio::test]
async fn simple_config_to_pipeline_config_aggregation() {
  let advanced_config = r#"
admin:
  bind: "localhost:9999"

pipeline:
  inflows:
    tcp:
      routes: ["processor:populate_cache"]
      tcp:
        bind: "[::]:3199"
        protocol:
          statsd: {}
        advanced:
          idle_timeout: "99s"
          buffer_size: 16384

    prom_remote_write:
      routes: ["processor:populate_cache"]
      prom_remote_write:
        bind: "[::]:3200"

  processors:
    populate_cache:
      routes: ["processor:aggregation"]
      populate_cache: {}

    aggregation:
      routes: ["processor:elision", "processor:saas_sampler"]
      aggregation:
        flush_interval: "15s"
        quantile_timers:
          quantiles: [0.5, 0.9, 0.99, 0.999]

    elision:
      routes: ["outflow:tcp"]
      elision:
        emit: { ratio: 0.1 }
        analyze_mode: true

    saas_sampler:
      routes: ["outflow:saas"]
      sampler:
        emit_interval: 3600s

  outflows:
    tcp:
      tcp:
        common:
          send_to: "localhost:3198"
          protocol:
            statsd: {}
          write_timeout: "30s"

    saas:
      prom_remote_write:
        send_to: "foo.com/v1/mme/ingest"
        metadata_only: true
        auth:
          bearer_token: { token: "1234" }
"#;
  let mut tf = NamedTempFile::new().unwrap();
  tf.write_all(advanced_config.as_bytes()).unwrap();
  load_and_check_validity(tf.path().to_str().unwrap())
    .await
    .unwrap();
}

#[tokio::test]
async fn invalid_route() {
  let config_yaml = r#"
pipeline:
  inflows:
    carbon_tcp_server:
      routes: ["processor:elision"]
      tcp:
        bind: "[::]:3200"
        protocol:
          carbon: {}
        advanced:
          idle_timeout: "600s"
          buffer_size: 10000

    prom_remote_write_server:
      routes: ["processor:elision"]
      prom_remote_write:
        bind: "[::]:3201"

  processors:
    elision:
      routes: ["outflow:bad_route"]
      elision:
        emit: { ratio: 0.1 }

  outflows:
    wavefront_proxy:
      tcp:
        common:
          send_to: "1.1.1.1:3200"
          protocol:
            carbon: {}
"#;
  let mut tf = NamedTempFile::new().unwrap();
  tf.write_all(config_yaml.as_bytes()).unwrap();
  assert_eq!(
    "invalid routing destination outflow:bad_route",
    load_and_check_validity(tf.path().to_str().unwrap())
      .await
      .unwrap_err()
      .to_string()
  );
}
