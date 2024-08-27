// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use bd_server_stats::stats::Collector;
use bd_shutdown::ComponentShutdownTrigger;
use bd_time::{TimeDurationExt, ToProtoDuration};
use prom_remote_write::PromRemoteWriteClientConfig;
use pulse_common::global_initialize;
use pulse_metrics::metric_generator::MetricGenerator;
use pulse_metrics::pipeline::outflow::prom::remote_write::PromRemoteWriteOutflow;
use pulse_metrics::pipeline::outflow::{OutflowFactoryContext, OutflowStats, PipelineOutflow};
use pulse_metrics::protos::metric::{DownstreamId, MetricValue, ParsedMetric};
use pulse_metrics::test::make_carbon_wire_protocol;
use pulse_protobuf::protos::pulse::config::outflow::v1::prom_remote_write;
use std::time::{Instant, UNIX_EPOCH};
use time::ext::NumericalDuration;

// TODO(mattklein123): Implement histogram support.

fn advance_metrics(metrics: &[ParsedMetric], value: f64) -> Vec<ParsedMetric> {
  let now = std::time::SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_secs();

  metrics
    .iter()
    .map(|parsed_metric| {
      let mut metric = parsed_metric.metric().clone();
      metric.timestamp = now;
      metric.value = MetricValue::Simple(value);
      ParsedMetric::new(
        metric,
        parsed_metric.source().clone(),
        Instant::now(),
        DownstreamId::LocalOrigin,
      )
    })
    .collect()
}

#[tokio::main]
pub async fn main() {
  global_initialize();

  let config = PromRemoteWriteClientConfig {
    send_to: "http://localhost:9009/api/v1/push".into(),
    max_in_flight: Some(16),
    request_timeout: 10.seconds().into_proto(),
    metadata_only: false,
    auth: None.into(),
    ..Default::default()
  };

  let scope = Collector::default().scope("pulse_promloadgen");

  let outflow_stats = OutflowStats::new(&scope, "generator");
  let shutdown_trigger = ComponentShutdownTrigger::default();
  let outflow = PromRemoteWriteOutflow::new(
    config,
    OutflowFactoryContext {
      name: "generator".to_owned(),
      stats: outflow_stats,
      shutdown_trigger_handle: shutdown_trigger.make_handle(),
    },
  )
  .await
  .unwrap();

  let mut generator = MetricGenerator::new(":");

  let sleep_interval = 60;
  let metrics_count = 1_000_000;
  let elided_metrics_count = 1_0;
  let chunk_count = 10;

  log::info!("generating names (this is slow, sorry)");
  let all_metrics = generator.generate_metrics(
    metrics_count + elided_metrics_count,
    &make_carbon_wire_protocol(),
  );
  let metrics = all_metrics[0 .. metrics_count].to_owned();
  let elided_metrics = all_metrics[metrics_count ..].to_owned();
  let mut value = 0;

  log::info!("done generating names");

  log::info!("example names for metrics: {:?}", metrics.first());
  log::info!(
    "example names for elided_metrics: {:?}",
    elided_metrics.first()
  );

  let chunk_size = elided_metrics.len() / chunk_count;
  loop {
    // Normal metrics
    let c = advance_metrics(metrics.as_ref(), value as f64);

    // Elided metrics
    let ce: Vec<_> = advance_metrics(
      elided_metrics.chunks(chunk_size).nth(value % 10).unwrap(),
      value as f64,
    );

    log::info!("sending {} samples", c.len());
    outflow.recv_samples(c).await;
    log::info!(
      "sending {} elided samples ({})",
      ce.len(),
      ce[0].metric().get_id()
    );
    outflow.recv_samples(ce).await;
    log::info!("send done for {} (chunk {}), sleep", value, value % 10);
    value += 1;
    sleep_interval.seconds().sleep().await;
  }
}
