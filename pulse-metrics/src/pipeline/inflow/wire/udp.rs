// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::util::{parse_lines, process_buffer_newlines};
use crate::pipeline::inflow::wire::util::bind_k8s_metadata;
use crate::pipeline::inflow::PipelineInflow;
use crate::pipeline::{ComponentShutdown, InflowFactoryContext, PipelineDispatch};
use crate::protos::metric::DownstreamId;
use async_trait::async_trait;
use bd_server_stats::stats::Scope;
use bd_shutdown::ComponentShutdownTriggerHandle;
use bytes::BytesMut;
use log::{info, warn};
use parking_lot::Mutex;
use prometheus::IntCounter;
use pulse_common::k8s::pods_info::OwnedPodsInfoSingleton;
use pulse_protobuf::protos::pulse::config::inflow::v1::wire::UdpServerConfig;
use std::sync::Arc;
use std::time::Instant;
use tokio::net::UdpSocket;
use tokio::select;

#[derive(Clone, Debug)]
struct UdpServerStats {
  incoming_bytes: IntCounter,
  unparsable: IntCounter,
  no_k8s_pod_metadata: IntCounter,
}

impl UdpServerStats {
  fn new(stats: &Scope) -> Self {
    Self {
      incoming_bytes: stats.counter("incoming_bytes"),
      unparsable: stats.counter("unparsable"),
      no_k8s_pod_metadata: stats.counter("no_k8s_pod_metadata"),
    }
  }
}

pub struct UdpInflow {
  scope: Scope,
  config: UdpServerConfig,
  dispatcher: Arc<dyn PipelineDispatch>,
  shutdown_trigger_handle: ComponentShutdownTriggerHandle,
  socket: Mutex<Option<UdpSocket>>,
  k8s_pods_info: Option<OwnedPodsInfoSingleton>,
}

impl UdpInflow {
  pub async fn new(config: UdpServerConfig, context: InflowFactoryContext) -> anyhow::Result<Self> {
    let k8s_pods_info = if config.bind_k8s_pod_metadata_by_remote_ip {
      Some((context.k8s_watch_factory)().await?.make_owned())
    } else {
      None
    };

    let socket = context
      .bind_resolver
      .resolve_udp(config.bind.as_str())
      .await?;
    info!("udp server running on {}", socket.local_addr().unwrap());
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
impl PipelineInflow for UdpInflow {
  async fn start(self: Arc<Self>) {
    let stats = UdpServerStats::new(&self.scope);

    tokio::spawn(udp_reader(
      stats,
      self.config.clone(),
      self.socket.lock().take().unwrap(),
      self.dispatcher.clone(),
      self.k8s_pods_info.clone(),
      self.shutdown_trigger_handle.make_shutdown(),
    ));
  }
}

async fn udp_reader(
  stats: UdpServerStats,
  config: UdpServerConfig,
  socket: UdpSocket,
  dispatcher: Arc<dyn PipelineDispatch>,
  k8s_pods_info: Option<OwnedPodsInfoSingleton>,
  mut shutdown: ComponentShutdown,
) {
  let buffer_size = 65507;
  let mut buf = BytesMut::with_capacity(buffer_size);
  loop {
    buf.resize(buffer_size, 0);
    select! {
      res = socket.recv_from(buf.as_mut()) => {
        let received_at = Instant::now();
        match res {
          Ok((bytes, peer_addr)) => {
            log::trace!("udp recv from={peer_addr} len={bytes}");
            stats.incoming_bytes.inc_by(bytes.try_into().unwrap());
            buf.truncate(bytes);
            let lines = process_buffer_newlines(&mut buf, false);
            let downstream_id = DownstreamId::IpAddress(peer_addr.ip());
            let mut parsed_lines = parse_lines(
              lines,
              &config.protocol,
              received_at,
              &downstream_id,
              &stats.unparsable,
            );
            bind_k8s_metadata(
              &mut parsed_lines,
              &downstream_id,
              &stats.no_k8s_pod_metadata,
              k8s_pods_info.as_ref(),
            );
            dispatcher.send(parsed_lines).await;
          },
          Err(e) => warn!("udp receiver error: {}", e),
        }
      }
      () = shutdown.cancelled() => {
        break;
      }
    }
  }
  info!("terminated udp server running on {}", config.bind);
  drop(shutdown);
}
