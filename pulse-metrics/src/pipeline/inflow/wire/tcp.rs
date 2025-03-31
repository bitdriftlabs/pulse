// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::pipeline::inflow::PipelineInflow;
use crate::pipeline::inflow::wire::util::{
  PreBufferConfig,
  SocketHandler,
  SocketHandlerConfig,
  SocketServerStats,
};
use crate::pipeline::{ComponentShutdown, InflowFactoryContext, PipelineDispatch};
use crate::protos::metric::DownstreamId;
use async_trait::async_trait;
use bd_server_stats::stats::Scope;
use bd_shutdown::ComponentShutdownTriggerHandle;
use bd_time::ProtoDurationExt;
use log::{debug, info};
use parking_lot::Mutex;
use pulse_common::bind_resolver::BoundTcpSocket;
use pulse_common::k8s::pods_info::OwnedPodsInfoSingleton;
use pulse_protobuf::protos::pulse::config::inflow::v1::wire::TcpServerConfig;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::select;

pub struct TcpInflow {
  scope: Scope,
  config: TcpServerConfig,
  dispatcher: Arc<dyn PipelineDispatch>,
  shutdown_trigger_handle: ComponentShutdownTriggerHandle,
  socket: Mutex<Option<BoundTcpSocket>>,
  k8s_pods_info: Option<OwnedPodsInfoSingleton>,
}

impl TcpInflow {
  pub async fn new(config: TcpServerConfig, context: InflowFactoryContext) -> anyhow::Result<Self> {
    let k8s_pods_info = if config.bind_k8s_pod_metadata_by_remote_ip {
      Some((context.k8s_watch_factory)().await?.make_owned())
    } else {
      None
    };

    let socket = context.bind_resolver.resolve_tcp(&config.bind).await?;
    info!("tcp server running on {}", socket.local_addr());
    Ok(Self {
      scope: context.scope,
      config,
      dispatcher: context.dispatcher,
      shutdown_trigger_handle: context.shutdown_trigger_handle,
      socket: Mutex::new(Some(socket)),
      k8s_pods_info,
    })
  }
}

#[async_trait]
impl PipelineInflow for TcpInflow {
  async fn start(self: Arc<Self>) {
    let stats = SocketServerStats::new(&self.scope);

    tokio::spawn(accept_tcp_connections(
      stats,
      self.config.clone(),
      self.socket.lock().take().unwrap().listen(),
      self.dispatcher.clone(),
      self.k8s_pods_info.clone(),
      self.shutdown_trigger_handle.make_shutdown(),
    ));
  }
}

async fn accept_tcp_connections(
  stats: SocketServerStats,
  config: TcpServerConfig,
  listener: TcpListener,
  dispatcher: Arc<dyn PipelineDispatch>,
  k8s_pods_info: Option<OwnedPodsInfoSingleton>,
  mut shutdown: ComponentShutdown,
) {
  let handler_config = SocketHandlerConfig {
    wire_protocol: config.protocol.unwrap(),
    advanced: config.advanced.unwrap_or_default(),
  };
  loop {
    select! {
      res = listener.accept() => {
        match res {
          Ok((socket, peer_addr)) => {
            debug!("accepted tcp connection from {peer_addr:?}");
            stats.accepts.inc();
            tokio::spawn(SocketHandler::new(
              stats.clone(),
              handler_config.clone(),
              DownstreamId::IpAddress(peer_addr.ip()),
              dispatcher.clone(),
              k8s_pods_info.clone(),
              config.pre_buffer_window.as_ref().map(|w| PreBufferConfig {
                timeout: w.to_time_duration(),
                always_pre_buffer: config.always_pre_buffer,
                reservoir_size: config.reservoir_size,
              }),
            ).run(socket, shutdown.clone()));
          }
          Err(e) => {
            info!("tcp accept error: {e}");
            stats.accept_failures.inc();
          }
        }
      },
      () = shutdown.cancelled() => {
        break;
      },
    };
  }
  info!("terminated tcp server running on {}", config.bind);
  drop(shutdown);
}
