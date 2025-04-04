// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use self::prom::remote_write::PromRemoteWriteOutflow;
use self::wire::{WireOutflow, WireOutflowClient};
use crate::clients::client::ConnectTo;
use crate::clients::client_pool;
use crate::protos::metric::ParsedMetric;
use anyhow::bail;
use async_trait::async_trait;
use bd_server_stats::stats::Scope;
use bd_shutdown::ComponentShutdownTriggerHandle;
use bd_time::ProtoDurationExt;
use prometheus::{Histogram, IntCounter};
use pulse_protobuf::protos::pulse::config::outflow::v1::outflow::OutflowConfig;
use pulse_protobuf::protos::pulse::config::outflow::v1::outflow::outflow_config::Config_type;
use pulse_protobuf::protos::pulse::config::outflow::v1::wire::{
  CommonWireClientConfig,
  NullClientConfig,
  TcpClientConfig,
  UdpClientConfig,
  UnixClientConfig,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::UdpSocket;

pub mod prom;
mod wire;

//
// OutflowStats
//

#[derive(Clone)]
pub struct OutflowStats {
  stats: Scope,
  messages_e2e_timer: Histogram,
  messages_outgoing_success: IntCounter,
  messages_outgoing_failed: IntCounter,
}

impl OutflowStats {
  #[must_use]
  pub fn new(stats: &Scope, outflow_name: &str) -> Self {
    let messages_e2e_timer = stats.histogram("messages_e2e_timer");

    let shared_labels = HashMap::from([
      ("src_type".to_string(), "outflow".to_string()),
      ("src".to_string(), outflow_name.to_string()),
    ]);

    let mut messages_outgoing_success_labels = shared_labels.clone();
    messages_outgoing_success_labels.insert("status".to_string(), "success".to_string());
    let messages_outgoing_success =
      stats.counter_with_labels("messages_outgoing", messages_outgoing_success_labels);

    let mut messages_outgoing_failed_labels = shared_labels;
    messages_outgoing_failed_labels.insert("status".to_string(), "failed".to_string());
    let messages_outgoing_failed =
      stats.counter_with_labels("messages_outgoing", messages_outgoing_failed_labels);

    Self {
      stats: stats.scope("outflow").scope(outflow_name),
      messages_e2e_timer,
      messages_outgoing_success,
      messages_outgoing_failed,
    }
  }

  pub fn messages_e2e_timer_observe(&self, received_at: &[Instant]) {
    let now = Instant::now();
    for received_at in received_at {
      if let Some(duration) = now.checked_duration_since(*received_at) {
        self.messages_e2e_timer.observe(duration.as_secs_f64());
      }
    }
  }
}

//
// PipelineOutflow
//

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait PipelineOutflow {
  /// Receive samples from the [crate::pipeline::MetricsPipeline] dispatch loop.
  /// Processing occurs asynchronously.
  async fn recv_samples(&self, samples: Vec<ParsedMetric>);
}

pub type DynamicPipelineOutflow = Arc<dyn PipelineOutflow + Send + Sync + 'static>;

//
// OutflowFactoryContext
//

pub struct OutflowFactoryContext {
  pub name: String,
  pub stats: OutflowStats,
  pub shutdown_trigger_handle: ComponentShutdownTriggerHandle,
}

pub async fn to_outflow(
  config: OutflowConfig,
  context: OutflowFactoryContext,
) -> anyhow::Result<DynamicPipelineOutflow> {
  // Match out the type of inflow config and dispatch it to the correct type
  match config.config_type.expect("pgv") {
    Config_type::Unix(config) => unix_outflow(config, context),
    Config_type::Tcp(config) => tcp_outflow(config, context),
    Config_type::Udp(config) => udp_outflow(config, context).await,
    Config_type::NullOutflow(config) => Ok(null_outflow(config, context)),
    Config_type::PromRemoteWrite(config) => Ok(PromRemoteWriteOutflow::new(config, context).await?),
  }
}

fn check_send_to(config: &CommonWireClientConfig) -> anyhow::Result<()> {
  if config.send_to.is_empty() {
    bail!("empty send_to");
  }
  Ok(())
}

fn unix_outflow(
  config: UnixClientConfig,
  context: OutflowFactoryContext,
) -> anyhow::Result<DynamicPipelineOutflow> {
  check_send_to(&config.common)?;
  let client = WireOutflowClient::Pool(client_pool::new(
    ConnectTo::UnixPath(config.common.send_to.to_string()),
    config
      .common
      .write_timeout
      .clone()
      .into_option()
      .map(|d| d.to_time_duration()),
  ));
  let outflow = WireOutflow::new(&config.common.unwrap(), client, context);
  Ok(Arc::new(outflow))
}

fn tcp_outflow(
  config: TcpClientConfig,
  context: OutflowFactoryContext,
) -> anyhow::Result<DynamicPipelineOutflow> {
  check_send_to(&config.common)?;
  let client = WireOutflowClient::Pool(client_pool::new(
    ConnectTo::TcpSocketAddr(config.common.send_to.to_string()),
    config
      .common
      .write_timeout
      .clone()
      .into_option()
      .map(|d| d.to_time_duration()),
  ));

  let outflow = WireOutflow::new(&config.common.unwrap(), client, context);
  Ok(Arc::new(outflow))
}

async fn udp_outflow(
  config: UdpClientConfig,
  context: OutflowFactoryContext,
) -> anyhow::Result<DynamicPipelineOutflow> {
  check_send_to(&config.common)?;
  let socket = Arc::new(UdpSocket::bind("[::]:0").await?);
  socket.connect(config.common.send_to.as_str()).await?;
  let client = WireOutflowClient::Udp(socket);
  let outflow = WireOutflow::new(&config.common.unwrap(), client, context);
  Ok(Arc::new(outflow))
}

fn null_outflow(
  config: NullClientConfig,
  context: OutflowFactoryContext,
) -> DynamicPipelineOutflow {
  let outflow = WireOutflow::new(
    &config.common.unwrap_or_default(),
    WireOutflowClient::Null(),
    context,
  );
  Arc::new(outflow)
}
