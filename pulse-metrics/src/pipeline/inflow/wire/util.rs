// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./util_test.rs"]
mod util_test;

use crate::pipeline::PipelineDispatch;
use crate::pipeline::inflow::wire::pre_buffer::PreBuffer;
use crate::pipeline::time::{DurationJitter, RealDurationJitter};
use crate::protos::metric::{DownstreamId, ParsedMetric};
use bd_log::warn_every;
use bd_server_stats::stats::{AutoGauge, Scope};
use bd_shutdown::ComponentShutdown;
use bd_time::{ProtoDurationExt, TimeDurationExt};
use bytes::{BufMut, Bytes, BytesMut};
use log::debug;
use memchr::memchr;
use prometheus::{IntCounter, IntGauge};
use pulse_common::k8s::pods_info::OwnedPodsInfoSingleton;
use pulse_common::proto::ProtoDurationToStdDuration;
use pulse_protobuf::protos::pulse::config::common::v1::common::WireProtocol;
use pulse_protobuf::protos::pulse::config::inflow::v1::wire::AdvancedSocketServerConfig;
use std::io::ErrorKind;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;
use time::Duration;
use time::ext::{NumericalDuration, NumericalStdDuration};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::{Sleep, timeout};
use tokio::{pin, select};

const fn default_idle_timeout() -> Duration {
  Duration::minutes(1)
}

const fn default_buffer_size() -> u64 {
  8192
}

#[derive(Clone, Debug)]
pub(super) struct SocketServerStats {
  pub accepts: IntCounter,
  pub accept_failures: IntCounter,
  pub incoming_bytes: IntCounter,
  pub disconnects: IntCounter,
  pub max_connection_duration: IntCounter,
  pub unparsable: IntCounter,
  pub no_k8s_pod_metadata: IntCounter,
  pub active: IntGauge,
}

//
// SocketServerStats
//

impl SocketServerStats {
  pub fn new(stats: &Scope) -> Self {
    Self {
      accepts: stats.counter("accepts"),
      accept_failures: stats.counter("accept_failures"),
      incoming_bytes: stats.counter("incoming_bytes"),
      disconnects: stats.counter("disconnects"),
      max_connection_duration: stats.counter("max_connection_duration"),
      unparsable: stats.counter("unparsable"),
      no_k8s_pod_metadata: stats.counter("no_k8s_pod_metadata"),
      active: stats.gauge("active"),
    }
  }
}

pub(super) fn process_buffer_newlines(
  buf: &mut BytesMut,
  expect_new_lines: bool,
) -> Vec<bytes::Bytes> {
  let mut ret: Vec<bytes::Bytes> = Vec::new();
  loop {
    match memchr(b'\n', buf) {
      None => break,
      Some(newline) => {
        let mut incoming = buf.split_to(newline + 1);
        let len = incoming.len();
        if len >= 2 && incoming[len - 2] == b'\r' {
          incoming.truncate(len - 2);
        } else if len >= 1 {
          incoming.truncate(len - 1);
        }
        let frozen = incoming.freeze();
        ret.push(frozen);
      },
    }
  }

  if !expect_new_lines && !buf.is_empty() {
    let frozen = buf.split().freeze();
    ret.push(frozen);
  }

  ret
}

pub(super) fn bind_k8s_metadata(
  metrics: &mut Vec<ParsedMetric>,
  downstream_id: &DownstreamId,
  no_k8s_pod_metadata: &IntCounter,
  k8s_pods_info: Option<&OwnedPodsInfoSingleton>,
) {
  if let (Some(k8s_pods_info), DownstreamId::IpAddress(ip_addr)) = (k8s_pods_info, downstream_id) {
    if let Some(remote_pod) = k8s_pods_info.borrow().by_ip(ip_addr) {
      for metric in metrics {
        metric.set_metadata(Some(remote_pod.metadata.clone()));
      }
    } else {
      warn_every!(
        1.minutes(),
        "dropping metrics from '{}' due to missing Kubernetes pod metadata",
        ip_addr
      );
      no_k8s_pod_metadata.inc_by(metrics.len().try_into().unwrap());
      metrics.clear();
    }
  }
}

pub(super) fn parse_lines(
  lines: Vec<bytes::Bytes>,
  wire_protocol: &WireProtocol,
  received_at: Instant,
  downstream_id: &DownstreamId,
  unparsable: &IntCounter,
) -> Vec<ParsedMetric> {
  let mut parsed = Vec::<ParsedMetric>::new();
  for line in lines {
    match ParsedMetric::try_from_wire_protocol(
      line.clone(),
      wire_protocol,
      received_at,
      downstream_id.clone(),
    ) {
      Ok(p) => {
        log::trace!("parsed metric '{}'", p.metric().get_id());
        parsed.push(p);
      },
      Err(e) => {
        warn_every!(
          15.seconds(),
          "parse failure {:?}. (original line: {:?}). source address: {:?}",
          e,
          line,
          downstream_id
        );

        unparsable.inc();
      },
    }
  }
  parsed
}

//
// SocketHandlerConfig
//

#[derive(Clone)]
pub(super) struct SocketHandlerConfig {
  pub wire_protocol: WireProtocol,
  pub advanced: AdvancedSocketServerConfig,
}

//
// PreBufferWrapper
//

pub(super) struct PreBufferWrapper {
  pub(super) config: PreBufferConfig,
  pub(super) buffer: PreBuffer,
  pub(super) sleep: Pin<Box<Sleep>>,
}

impl PreBufferWrapper {
  pub(super) fn new(config: PreBufferConfig) -> Self {
    let buffer = PreBuffer::new(config.reservoir_size);
    let sleep = Box::pin(config.timeout.sleep());
    Self {
      config,
      buffer,
      sleep,
    }
  }

  pub(super) fn reset(&mut self) -> PreBuffer {
    let old_buffer =
      std::mem::replace(&mut self.buffer, PreBuffer::new(self.config.reservoir_size));
    self
      .sleep
      .as_mut()
      .reset(tokio::time::Instant::now() + self.config.timeout.unsigned_abs());
    old_buffer
  }

  pub(super) async fn flush_pre_buffer(
    pre_buffer: &mut Option<Self>,
    downstream_id: &DownstreamId,
    no_k8s_pod_metadata: &IntCounter,
    k8s_pods_info: Option<&OwnedPodsInfoSingleton>,
    dispatcher: &dyn PipelineDispatch,
  ) {
    let pre_buffer = {
      if pre_buffer.as_ref().unwrap().config.always_pre_buffer {
        pre_buffer.as_mut().unwrap().reset()
      } else {
        pre_buffer.take().unwrap().buffer
      }
    };
    let mut metrics = pre_buffer.flush(downstream_id);
    log::debug!("flushed prebuffer with {} metrics", metrics.len());
    bind_k8s_metadata(
      &mut metrics,
      downstream_id,
      no_k8s_pod_metadata,
      k8s_pods_info,
    );
    dispatcher.send(metrics).await;
  }
}

//
// PreBufferConfig
//

#[derive(Clone)]
pub(super) struct PreBufferConfig {
  pub timeout: Duration,
  pub always_pre_buffer: bool,
  pub reservoir_size: Option<u32>,
}

//
// SocketHandler
//

pub(super) struct SocketHandler {
  stats: SocketServerStats,
  config: SocketHandlerConfig,
  downstream_id: DownstreamId,
  dispatcher: Arc<dyn PipelineDispatch>,
  k8s_pods_info: Option<OwnedPodsInfoSingleton>,
  pre_buffer: Option<PreBufferWrapper>,
}

impl SocketHandler {
  pub(super) fn new(
    stats: SocketServerStats,
    config: SocketHandlerConfig,
    downstream_id: DownstreamId,
    dispatcher: Arc<dyn PipelineDispatch>,
    k8s_pods_info: Option<OwnedPodsInfoSingleton>,
    pre_buffer_config: Option<PreBufferConfig>,
  ) -> Self {
    Self {
      stats,
      config,
      downstream_id,
      dispatcher,
      k8s_pods_info,
      pre_buffer: pre_buffer_config.map(PreBufferWrapper::new),
    }
  }

  async fn process(&mut self, lines: Vec<Bytes>) {
    let mut parsed_lines = parse_lines(
      lines,
      &self.config.wire_protocol,
      Instant::now(),
      &self.downstream_id,
      &self.stats.unparsable,
    );
    if let Some(pre_buffer) = &mut self.pre_buffer {
      pre_buffer.buffer.buffer(parsed_lines);
    } else {
      bind_k8s_metadata(
        &mut parsed_lines,
        &self.downstream_id,
        &self.stats.no_k8s_pod_metadata,
        self.k8s_pods_info.as_ref(),
      );
      self.dispatcher.send(parsed_lines).await;
    }
  }

  #[allow(clippy::cognitive_complexity)]
  pub(super) async fn run<T>(mut self, mut socket: T, mut shutdown: ComponentShutdown)
  where
    T: AsyncRead + AsyncWrite + Unpin,
  {
    // TODO(mattklein123): Add bounds on max active connections.
    let _active_auto_gauge = AutoGauge::new(self.stats.active.clone());
    let buffer_size = self
      .config
      .advanced
      .buffer_size
      .unwrap_or(default_buffer_size())
      .try_into()
      .unwrap();
    let mut buf = BytesMut::with_capacity(buffer_size);
    let mut max_duration_sleep =
      self
        .config
        .advanced
        .max_connection_duration
        .as_ref()
        .map(|max_connection_duration| {
          Box::pin(
            RealDurationJitter::half_jitter_duration(max_connection_duration.to_time_duration())
              .sleep(),
          )
        });
    let mut shutdown_wait = None;

    loop {
      if buf.remaining_mut() < buffer_size {
        buf.reserve(buffer_size);
      }

      let idle_timeout = self
        .config
        .advanced
        .idle_timeout
        .unwrap_duration_or(default_idle_timeout())
        .sleep();
      pin!(idle_timeout);
      let result = select! {
        r = socket.read_buf(&mut buf) => r,
        () = &mut idle_timeout => {
          Err(std::io::Error::new(ErrorKind::TimedOut, "read timeout"))
        }
        () = shutdown.cancelled() => {
          Err(std::io::Error::other("shutting down"))
        }
        () = async {
          self.pre_buffer.as_mut().unwrap().sleep.as_mut().await;
        }, if self.pre_buffer.is_some() => {
          PreBufferWrapper::flush_pre_buffer(
            &mut self.pre_buffer,
            &self.downstream_id,
            &self.stats.no_k8s_pod_metadata,
            self.k8s_pods_info.as_ref(),
            &*self.dispatcher,
          ).await;
          continue;
        }
        () = async {
          max_duration_sleep.as_mut().unwrap().await;
        }, if max_duration_sleep.is_some() => {
          // Shutdown and begin the wait process. This assumes the other side is correctly
          // monitoring for shutdown (typically waiting on recv() so it sees the TCP FIN that is
          // sent).
          log::debug!("max connection duration reached. Shutting down and beginning wait");
          self.stats.max_connection_duration.inc();
          let _ignored = socket.shutdown().await;
          shutdown_wait = Some(Box::pin(5.seconds().sleep()));
          max_duration_sleep = None;
          continue;
        }
        () = async { shutdown_wait.as_mut().unwrap().await }, if shutdown_wait.is_some() => {
          log::debug!("shutdown wait complete");
          Err(std::io::Error::new(ErrorKind::TimedOut, "max connection duration"))
        }
      };

      match result {
        Ok(bytes) if buf.is_empty() && bytes == 0 => {
          debug!(
            "closing reader (empty buffer, eof) {:?}",
            self.downstream_id
          );
          break;
        },
        Ok(0) => {
          // Socket shutdown, process what's remaining
          let mut lines = process_buffer_newlines(&mut buf, true);
          let remaining = buf.clone().freeze();
          lines.push(remaining);
          self.process(lines).await;
          debug!("remaining {buf:?}");
          debug!("closing reader {:?}", self.downstream_id);
          break;
        },
        Ok(bytes) => {
          self.stats.incoming_bytes.inc_by(bytes.try_into().unwrap());
          let lines = process_buffer_newlines(&mut buf, true);
          self.process(lines).await;
        },
        Err(e) if e.kind() == ErrorKind::Other => {
          // Ignoring the results of the write call here
          let _ignored = timeout(
            1.std_seconds(),
            socket.write_all(b"server closing due to shutdown, goodbye\n"),
          )
          .await;
          break;
        },
        Err(e) if e.kind() == ErrorKind::TimedOut => {
          debug!("read timeout, closing {:?}", self.downstream_id);
          break;
        },
        Err(e) => {
          debug!("socket error {:?} {:?}", e, self.downstream_id);
          break;
        },
      }
    }

    // If we lost the connection prior to the pre-buffer timeout, try to flush what we have.
    if self.pre_buffer.is_some() {
      PreBufferWrapper::flush_pre_buffer(
        &mut self.pre_buffer,
        &self.downstream_id,
        &self.stats.no_k8s_pod_metadata,
        self.k8s_pods_info.as_ref(),
        &*self.dispatcher,
      )
      .await;
    }

    self.stats.disconnects.inc();
    drop(shutdown);
  }
}
