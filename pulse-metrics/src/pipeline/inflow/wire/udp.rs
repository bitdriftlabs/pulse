// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::util::{parse_lines, process_buffer_newlines, PreBufferConfig, PreBufferWrapper};
use crate::pipeline::inflow::wire::util::bind_k8s_metadata;
use crate::pipeline::inflow::PipelineInflow;
use crate::pipeline::{ComponentShutdown, InflowFactoryContext, PipelineDispatch};
use crate::protos::metric::DownstreamId;
use ahash::AHashMap;
use async_trait::async_trait;
use bd_server_stats::stats::Scope;
use bd_shutdown::ComponentShutdownTriggerHandle;
use bd_time::ProtoDurationExt;
use bytes::{Bytes, BytesMut};
use log::{info, warn};
use parking_lot::Mutex;
use prometheus::IntCounter;
use pulse_common::k8s::pods_info::OwnedPodsInfoSingleton;
use pulse_protobuf::protos::pulse::config::inflow::v1::wire::UdpServerConfig;
use std::collections::hash_map::Entry;
use std::mem::MaybeUninit;
use std::net::IpAddr;
use std::ptr;
use std::sync::Arc;
use std::time::Instant;
use time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::{sleep, Instant as TokioInstant};
use tokio::{pin, select};

struct Shared {
  stats: UdpServerStats,
  config: UdpServerConfig,
  dispatcher: Arc<dyn PipelineDispatch>,
  k8s_pods_info: Option<OwnedPodsInfoSingleton>,
  shutdown: ComponentShutdownTriggerHandle,
}

//
// SessionManager
//

#[async_trait]
trait SessionManager: Send + Sync {
  async fn recv_lines(&mut self, lines: Vec<Bytes>, peer_ip: IpAddr);
}

//
// DefaultSessionManager
//

struct DefaultSessionManager {
  shared: Arc<Shared>,
}

#[async_trait]
impl SessionManager for DefaultSessionManager {
  async fn recv_lines(&mut self, lines: Vec<Bytes>, peer_ip: IpAddr) {
    let received_at = Instant::now();
    let downstream_id = DownstreamId::IpAddress(peer_ip);
    let mut parsed_lines = parse_lines(
      lines,
      &self.shared.config.protocol,
      received_at,
      &downstream_id,
      &self.shared.stats.unparsable,
    );
    bind_k8s_metadata(
      &mut parsed_lines,
      &downstream_id,
      &self.shared.stats.no_k8s_pod_metadata,
      self.shared.k8s_pods_info.as_ref(),
    );
    self.shared.dispatcher.send(parsed_lines).await;
  }
}

//
// PreBufferSessionManager
//

struct PreBufferSessionManager {
  shared: Arc<Shared>,
  sessions: Arc<Mutex<AHashMap<IpAddr, mpsc::Sender<Vec<Bytes>>>>>,
  pre_buffer_config: PreBufferConfig,
  session_idle_timeout: Duration,
}

#[async_trait]
impl SessionManager for PreBufferSessionManager {
  async fn recv_lines(&mut self, lines: Vec<Bytes>, peer_ip: IpAddr) {
    let session = self
      .sessions
      .lock()
      .entry(peer_ip)
      .or_insert_with(|| {
        let (tx, rx) = mpsc::channel(1);
        log::debug!("creating new session for {}", peer_ip);
        tokio::spawn(
          PreBufferSession::new(
            self.shared.clone(),
            self.sessions.clone(),
            peer_ip,
            self.pre_buffer_config.clone(),
          )
          .run(
            rx,
            self.shared.shutdown.make_shutdown(),
            self.session_idle_timeout,
          ),
        );
        tx
      })
      .clone();
    // We guarantee that the receiver is never removed if the strong count is greater than 1. Thus
    // this should never fail.
    session.send(lines).await.unwrap();
  }
}

//
// PreBufferSession
//

struct PreBufferSession {
  shared: Arc<Shared>,
  sessions: Arc<Mutex<AHashMap<IpAddr, mpsc::Sender<Vec<Bytes>>>>>,
  peer_ip: IpAddr,
  pre_buffer: Option<PreBufferWrapper>,
}

impl PreBufferSession {
  fn new(
    shared: Arc<Shared>,
    sessions: Arc<Mutex<AHashMap<IpAddr, mpsc::Sender<Vec<Bytes>>>>>,
    peer_ip: IpAddr,
    pre_buffer_config: PreBufferConfig,
  ) -> Self {
    Self {
      shared,
      sessions,
      peer_ip,
      pre_buffer: Some(PreBufferWrapper::new(pre_buffer_config)),
    }
  }

  async fn run(
    mut self,
    mut rx: mpsc::Receiver<Vec<Bytes>>,
    mut shutdown: ComponentShutdown,
    session_idle_timeout: Duration,
  ) {
    let idle_timeout = sleep(session_idle_timeout.unsigned_abs());
    pin!(idle_timeout);
    let shutdown_future = shutdown.cancelled();
    pin!(shutdown_future);

    loop {
      idle_timeout
        .as_mut()
        .reset(TokioInstant::now() + session_idle_timeout.unsigned_abs());

      select! {
        lines = rx.recv() => {
          // We control when things leave the map so there should always be a sender.
          self.process(lines.unwrap()).await;
        },
        () = async {
          self.pre_buffer.as_mut().unwrap().sleep.as_mut().await;
        }, if self.pre_buffer.is_some() => {
          self.flush_pre_buffer().await;
          continue;
        },
        () = &mut idle_timeout => {
          if self.on_idle_timeout() {
            log::debug!("session idle timeout for {}", self.peer_ip);
            break;
          }
          continue;
        },
        () = &mut shutdown_future => {
          break;
        }
      }
    }

    // If we lost the connection prior to the pre-buffer timeout, try to flush what we have.
    if self.pre_buffer.is_some() {
      self.flush_pre_buffer().await;
    }
  }

  fn on_idle_timeout(&self) -> bool {
    let mut sessions = self.sessions.lock();
    if let Entry::Occupied(o) = sessions.entry(self.peer_ip) {
      // If the strong count is 1 under lock, we have the only reference, and thus it is safe to
      // remove from the map. Any new packets will create a new session with a new channel. If
      // however the strong count is > 1 this means that there was a race and a packet is in the
      // process of being sent to this channel. Keep the channel open and loop around again.
      if o.get().strong_count() == 1 {
        o.remove();
        return true;
      }
      return false;
    }
    true
  }

  async fn process(&mut self, lines: Vec<Bytes>) {
    let downstream_id = DownstreamId::IpAddress(self.peer_ip);
    let mut parsed_lines = parse_lines(
      lines,
      &self.shared.config.protocol,
      Instant::now(),
      &downstream_id,
      &self.shared.stats.unparsable,
    );
    if let Some(pre_buffer) = &mut self.pre_buffer {
      pre_buffer.buffer.buffer(parsed_lines);
    } else {
      bind_k8s_metadata(
        &mut parsed_lines,
        &downstream_id,
        &self.shared.stats.no_k8s_pod_metadata,
        self.shared.k8s_pods_info.as_ref(),
      );
      self.shared.dispatcher.send(parsed_lines).await;
    }
  }

  async fn flush_pre_buffer(&mut self) {
    let downstream_id = DownstreamId::IpAddress(self.peer_ip);
    PreBufferWrapper::flush_pre_buffer(
      &mut self.pre_buffer,
      &downstream_id,
      &self.shared.stats.no_k8s_pod_metadata,
      self.shared.k8s_pods_info.as_ref(),
      &*self.shared.dispatcher,
    )
    .await;
  }
}

//
// UdpServerStats
//

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

//
// UdpInflow
//

pub struct UdpInflow {
  shared: Arc<Shared>,
  socket: Mutex<Option<UdpSocket>>,
}

impl UdpInflow {
  pub async fn new(config: UdpServerConfig, context: InflowFactoryContext) -> anyhow::Result<Self> {
    let k8s_pods_info = if config.bind_k8s_pod_metadata_by_remote_ip {
      Some((context.k8s_watch_factory)().await?.make_owned())
    } else {
      None
    };

    let stats = UdpServerStats::new(&context.scope);
    let shared = Arc::new(Shared {
      stats,
      config,
      dispatcher: context.dispatcher,
      k8s_pods_info,
      shutdown: context.shutdown_trigger_handle,
    });

    let socket = context
      .bind_resolver
      .resolve_udp(shared.config.bind.as_str())
      .await?;
    info!("udp server running on {}", socket.local_addr().unwrap());
    Ok(Self {
      shared,
      socket: Mutex::new(Some(socket)),
    })
  }
}

#[async_trait]
impl PipelineInflow for UdpInflow {
  async fn start(self: Arc<Self>) {
    let session_manager = self.shared.config.pre_buffer.as_ref().map_or_else(
      || {
        Box::new(DefaultSessionManager {
          shared: self.shared.clone(),
        }) as Box<dyn SessionManager>
      },
      |pre_buffer| {
        Box::new(PreBufferSessionManager {
          shared: self.shared.clone(),
          sessions: Arc::default(),
          pre_buffer_config: PreBufferConfig {
            timeout: pre_buffer.pre_buffer_window.to_time_duration(),
            always_pre_buffer: pre_buffer.always_pre_buffer,
          },
          session_idle_timeout: pre_buffer.session_idle_timeout.to_time_duration(),
        }) as Box<dyn SessionManager>
      },
    );

    tokio::spawn(udp_reader(
      self.shared.clone(),
      self.socket.lock().take().unwrap(),
      session_manager,
      self.shared.shutdown.make_shutdown(),
    ));
  }
}

async fn udp_reader(
  shared: Arc<Shared>,
  socket: UdpSocket,
  mut session_manager: Box<dyn SessionManager>,
  mut shutdown: ComponentShutdown,
) {
  let buffer_size = 65507;
  let mut buf = BytesMut::with_capacity(buffer_size);
  loop {
    buf.reserve(buffer_size);
    select! {
      res = socket.recv_from(
        unsafe {
          &mut *(ptr::from_mut::<[MaybeUninit<u8>]>(buf.spare_capacity_mut()) as *mut [u8])
        }) => {
        match res {
          Ok((bytes, peer_addr)) => {
            log::trace!("udp recv from={peer_addr} len={bytes}");
            shared.stats.incoming_bytes.inc_by(bytes.try_into().unwrap());
            unsafe { buf.set_len(bytes); }
            let lines = process_buffer_newlines(&mut buf, false);
            debug_assert!(buf.is_empty());
            session_manager.recv_lines(lines, peer_addr.ip()).await;
          },
          Err(e) => warn!("udp receiver error: {}", e),
        }
      }
      () = shutdown.cancelled() => {
        break;
      }
    }
  }
  info!("terminated udp server running on {}", shared.config.bind);
  drop(shutdown);
}
