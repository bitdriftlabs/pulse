// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::util::SocketServerStats;
use crate::pipeline::inflow::wire::util::{SocketHandler, SocketHandlerConfig};
use crate::pipeline::inflow::{InflowFactoryContext, PipelineInflow};
use crate::pipeline::{ComponentShutdown, PipelineDispatch};
use crate::protos::metric::DownstreamId;
use async_trait::async_trait;
use bd_server_stats::stats::Scope;
use bd_shutdown::ComponentShutdownTriggerHandle;
use log::{debug, info};
use parking_lot::Mutex;
use pulse_protobuf::protos::pulse::config::inflow::v1::wire::UnixServerConfig;
use std::sync::Arc;
use tokio::net::UnixListener;
use tokio::select;

pub struct UnixInflow {
  scope: Scope,
  config: UnixServerConfig,
  dispatcher: Arc<dyn PipelineDispatch>,
  shutdown_trigger_handle: ComponentShutdownTriggerHandle,
  listener: Mutex<Option<UnixListener>>,
}

impl UnixInflow {
  pub fn new(config: UnixServerConfig, context: InflowFactoryContext) -> anyhow::Result<Self> {
    let listener = UnixListener::bind(&config.path)?;
    info!("unix server running on {}", config.path);
    Ok(Self {
      scope: context.scope,
      config,
      dispatcher: context.dispatcher,
      shutdown_trigger_handle: context.shutdown_trigger_handle,
      listener: Mutex::new(Some(listener)),
    })
  }
}

#[async_trait]
impl PipelineInflow for UnixInflow {
  async fn start(self: Arc<Self>) {
    let stats = SocketServerStats::new(&self.scope);

    tokio::spawn(accept_unix_connections(
      stats,
      self.config.clone(),
      self.listener.lock().take().unwrap(),
      self.dispatcher.clone(),
      self.shutdown_trigger_handle.make_shutdown(),
    ));
  }
}

async fn accept_unix_connections(
  stats: SocketServerStats,
  config: UnixServerConfig,
  listener: UnixListener,
  dispatcher: Arc<dyn PipelineDispatch>,
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
            debug!("accepted unix connection from {peer_addr:?}");
            stats.accepts.inc();
            tokio::spawn(SocketHandler::new(
              stats.clone(),
              handler_config.clone(),
              DownstreamId::UnixDomainSocket(peer_addr
                .as_pathname()
                .map(|p| p.display().to_string())
                .unwrap_or_default().into()),
              dispatcher.clone(),
              None,
              None,
            ).run(socket, shutdown.clone()));
          },
          Err(e) => {
            info!("unix accept error: {e}");
            stats.accept_failures.inc();
          },
        }
      },
      () = shutdown.cancelled() =>  {
        break;
      },
    };
  }
  drop(listener);
  // The socket file descriptor is not removed on teardown. Let's remove it now.
  let _ignored = std::fs::remove_file(config.path.clone());
  info!("terminated unix server running on {}", config.path);
  drop(shutdown);
}
