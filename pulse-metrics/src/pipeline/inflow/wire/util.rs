// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./util_test.rs"]
mod util_test;

use crate::pipeline::time::{DurationJitter, RealDurationJitter};
use crate::pipeline::PipelineDispatch;
use crate::protos::metric::{DownstreamId, ParsedMetric};
use bd_log::warn_every;
use bd_server_stats::stats::{AutoGauge, Scope};
use bd_shutdown::ComponentShutdown;
use bd_time::{ProtoDurationExt, TimeDurationExt};
use bytes::{BufMut, BytesMut};
use log::debug;
use memchr::memchr;
use prometheus::{IntCounter, IntGauge};
use pulse_common::k8s::pods_info::OwnedPodsInfoSingleton;
use pulse_common::proto::ProtoDurationToStdDuration;
use pulse_protobuf::protos::pulse::config::common::v1::common::WireProtocol;
use pulse_protobuf::protos::pulse::config::inflow::v1::wire::AdvancedSocketServerConfig;
use std::io::ErrorKind;
use std::sync::Arc;
use std::time::Instant;
use time::ext::{NumericalDuration, NumericalStdDuration};
use time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::timeout;
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
    };
  }

  if !expect_new_lines && !buf.is_empty() {
    let frozen = buf.split().freeze();
    ret.push(frozen);
  }

  ret
}

pub(super) fn parse_lines(
  lines: Vec<bytes::Bytes>,
  wire_protocol: &WireProtocol,
  received_at: Instant,
  downstream_id: &DownstreamId,
  unparsable: &IntCounter,
  no_k8s_pod_metadata: &IntCounter,
  k8s_pods_info: Option<&OwnedPodsInfoSingleton>,
) -> Vec<ParsedMetric> {
  // TODO(mattklein123): Technically, for TCP we should be able to get away with looking up the
  // metadata once and then continuing to use it for the entire connection. However, there are
  // edge cases where it's possible that we get a connection before we know about the pod from
  // the API server. In this case we would have to allow for late load. For now just look it up
  // every time. We can relax this later.
  let mut metadata = None;
  if let (Some(k8s_pods_info), DownstreamId::IpAddress(ip_addr)) = (k8s_pods_info, downstream_id) {
    if let Some(remote_pod) = k8s_pods_info.borrow().by_ip(ip_addr) {
      metadata = Some(remote_pod.metadata.clone());
    } else {
      warn_every!(
        1.minutes(),
        "dropping metrics from '{}' due to missing Kubernetes pod metadata",
        ip_addr
      );
      no_k8s_pod_metadata.inc_by(lines.len().try_into().unwrap());
      return vec![];
    }
  }

  let mut parsed = Vec::<ParsedMetric>::new();
  for line in lines {
    match ParsedMetric::try_from_wire_protocol(
      line.clone(),
      wire_protocol,
      received_at,
      downstream_id.clone(),
    ) {
      Ok(mut p) => {
        p.set_metadata(metadata.clone());
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
    };
  }
  parsed
}

#[derive(Clone)]
pub(super) struct SocketHandlerConfig {
  pub wire_protocol: WireProtocol,
  pub advanced: AdvancedSocketServerConfig,
}

pub(super) async fn socket_handler<T>(
  stats: SocketServerStats,
  config: SocketHandlerConfig,
  mut socket: T,
  downstream_id: DownstreamId,
  dispatcher: Arc<dyn PipelineDispatch>,
  k8s_pods_info: Option<OwnedPodsInfoSingleton>,
  mut shutdown: ComponentShutdown,
) where
  T: AsyncRead + AsyncWrite + Unpin,
{
  // TODO(mattklein123): Add bounds on max active connections.
  let _active_auto_gauge = AutoGauge::new(stats.active);
  let buffer_size = config
    .advanced
    .buffer_size
    .unwrap_or(default_buffer_size())
    .try_into()
    .unwrap();
  let mut buf = BytesMut::with_capacity(buffer_size);
  let mut max_duration_sleep =
    config
      .advanced
      .max_connection_duration
      .into_option()
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

    let idle_timeout = config
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
        Err(std::io::Error::new(ErrorKind::Other, "shutting down"))
      }
      () = async {
        max_duration_sleep.as_mut().unwrap().await;
      }, if max_duration_sleep.is_some() => {
        // Shutdown and begin the wait process. This assumes the other side is correctly
        // monitoring for shutdown (typically waiting on recv() so it sees the TCP FIN that is
        // sent).
        log::debug!("max connection duration reached. Shutting down and beginning wait");
        stats.max_connection_duration.inc();
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

    let received_at = Instant::now();
    match result {
      Ok(bytes) if buf.is_empty() && bytes == 0 => {
        debug!("closing reader (empty buffer, eof) {:?}", downstream_id);
        break;
      },
      Ok(0) => {
        // Socket shutdown, process what's remaining
        let mut lines = process_buffer_newlines(&mut buf, true);
        let remaining = buf.clone().freeze();
        lines.push(remaining);

        let parsed_lines = parse_lines(
          lines,
          &config.wire_protocol,
          received_at,
          &downstream_id,
          &stats.unparsable,
          &stats.no_k8s_pod_metadata,
          k8s_pods_info.as_ref(),
        );
        dispatcher.send(parsed_lines).await;

        debug!("remaining {:?}", buf);
        debug!("closing reader {:?}", downstream_id);
        break;
      },
      Ok(bytes) => {
        stats.incoming_bytes.inc_by(bytes.try_into().unwrap());
        let lines = process_buffer_newlines(&mut buf, true);

        let parsed_lines = parse_lines(
          lines,
          &config.wire_protocol,
          received_at,
          &downstream_id,
          &stats.unparsable,
          &stats.no_k8s_pod_metadata,
          k8s_pods_info.as_ref(),
        );
        dispatcher.send(parsed_lines).await;
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
        debug!("read timeout, closing {:?}", downstream_id);
        break;
      },
      Err(e) => {
        debug!("socket error {:?} {:?}", e, downstream_id);
        break;
      },
    }
  }
  stats.disconnects.inc();
  drop(shutdown);
}
