// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./mod_test.rs"]
mod mod_test;

use super::{InflowFactoryContext, PipelineInflow};
use crate::pipeline::PipelineDispatch;
use crate::protos::metric::{DownstreamId, DownstreamIdProvider, MetricId, ParsedMetric};
use async_trait::async_trait;
use axum::Router;
use axum::extract::{ConnectInfo, Request, State};
use axum::response::Response;
use axum::routing::{get, post};
use bd_grpc::axum_helper::serve_with_connect_info;
use bd_log::warn_every;
use bd_proto::protos::prometheus::prompb::remote::WriteRequest;
use bd_server_stats::stats::{AutoGauge, Scope};
use bd_shutdown::ComponentShutdownTriggerHandle;
use bytes::{BufMut, Bytes, BytesMut};
use http::StatusCode;
use http_body_util::BodyExt;
use hyper::body::Body;
use itertools::Itertools;
use log::info;
use parking_lot::Mutex;
use prom_remote_write::PromRemoteWriteServerConfig;
use prom_remote_write::prom_remote_write_server_config::DownstreamIdSource;
use prom_remote_write::prom_remote_write_server_config::downstream_id_source::Source_type;
use prometheus::{IntCounter, IntGauge};
use protobuf::Message;
use pulse_common::bind_resolver::BoundTcpSocket;
use pulse_protobuf::protos::pulse::config::inflow::v1::prom_remote_write;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Instant;
use time::ext::NumericalDuration;

const MAX_ALLOWED_REQUEST_SIZE: u64 = 20_000_000;

//
// Stats
//

#[derive(Clone)]
struct Stats {
  connections_total: IntCounter,
  connections_active: IntGauge,
  requests_active: IntGauge,
  requests_total: IntCounter,
  requests_4xx: IntCounter,
}

// TODO(mattklein123): Break down 4xx by error type.
impl Stats {
  pub fn new(scope: &Scope) -> Self {
    Self {
      connections_total: scope.counter("connections_total"),
      connections_active: scope.gauge("connections_active"),
      requests_active: scope.gauge("requests_active"),
      requests_total: scope.counter("requests_total"),
      requests_4xx: scope.counter("requests_4xx"),
    }
  }
}

//
// PromRemoteWriteInflow
//

pub(super) struct PromRemoteWriteInflow {
  stats: Stats,
  config: PromRemoteWriteServerConfig,
  dispatcher: Arc<dyn PipelineDispatch>,
  shutdown_trigger_handle: ComponentShutdownTriggerHandle,
  socket: Mutex<Option<BoundTcpSocket>>,
}

impl PromRemoteWriteInflow {
  pub async fn new(
    config: PromRemoteWriteServerConfig,
    context: InflowFactoryContext,
  ) -> anyhow::Result<Self> {
    let stats = Stats::new(&context.scope);
    let socket = context.bind_resolver.resolve_tcp(&config.bind).await?;
    Ok(Self {
      stats,
      config,
      dispatcher: context.dispatcher,
      shutdown_trigger_handle: context.shutdown_trigger_handle,
      socket: Mutex::new(Some(socket)),
    })
  }
}

#[async_trait]
impl PipelineInflow for PromRemoteWriteInflow {
  async fn start(self: Arc<Self>) {
    let router = Router::new()
      .route("/healthcheck", get(|| async { "OK" }))
      .route("/api/v1/prom/write", post(prometheus_remote_write_handler))
      .with_state(self.clone());

    let socket = self.socket.lock().take().unwrap();
    info!(
      "prometheus remote write server starting at {}",
      socket.local_addr()
    );
    tokio::spawn(async move {
      let mut shutdown = self.shutdown_trigger_handle.make_shutdown();
      let _ignored = serve_with_connect_info(
        router,
        socket.listen(),
        self.stats.connections_total.clone(),
        self.stats.connections_active.clone(),
        shutdown.cancelled(),
      )
      .await;
      info!(
        "terminated prometheus remote write server running at {}",
        &self.config.bind
      );
    });
  }
}

//
// DecodeError
//

#[derive(thiserror::Error, Debug)]
enum DecodeError {
  #[error("io error: {0}")]
  IO(#[from] std::io::Error),
  #[error("protobuf decode error: {0}")]
  ProtobufDecode(#[from] protobuf::Error),
  #[error("snappy decode error: {0}")]
  SnappyDecode(#[from] snap::Error),
}

//
// DownstreamIdProviderImpl
//

struct DownstreamIdProviderImpl {
  base: Bytes,
  append_tags_to_downstream_id: bool,
}

impl DownstreamIdProviderImpl {
  fn new(
    append_tags_to_downstream_id: bool,
    downstream_id_source: Option<&DownstreamIdSource>,
    remote_addr: IpAddr,
    req: &Request,
  ) -> Self {
    // Note, there is currently no way to get the Bytes directly out of the header value so we have
    // to clone it.
    let base = downstream_id_source
      .and_then(|source| match source.source_type.as_ref().expect("pgv") {
        Source_type::RemoteIp(_) => None,
        Source_type::RequestHeader(header_name) => req
          .headers()
          .get(header_name.as_str())
          .map(|value| value.as_bytes().to_vec().into())
          .or_else(|| {
            warn_every!(
              1.minutes(),
              "downstream ID header '{}' not found",
              header_name
            );
            None
          }),
      })
      .unwrap_or_else(|| remote_addr.to_string().into());

    Self {
      base,
      append_tags_to_downstream_id,
    }
  }
}

impl DownstreamIdProvider for DownstreamIdProviderImpl {
  fn downstream_id(&self, metric_id: &MetricId) -> DownstreamId {
    let bytes = if self.append_tags_to_downstream_id {
      let mut bytes = BytesMut::with_capacity(self.base.len());
      bytes.extend_from_slice(&self.base);
      for tag in metric_id.tags() {
        bytes.put_u8(b':');
        bytes.extend_from_slice(&tag.tag);
        bytes.put_u8(b'=');
        bytes.extend_from_slice(&tag.value);
      }
      bytes.freeze()
    } else {
      self.base.clone()
    };
    DownstreamId::InflowProvided(bytes)
  }
}

fn decode_body_into_write_request(body: &[u8]) -> Result<WriteRequest, DecodeError> {
  let decompressed = snap::raw::Decoder::new().decompress_vec(body)?;
  let write_request = WriteRequest::parse_from_tokio_bytes(&Bytes::from(decompressed))?;
  Ok(write_request)
}

fn make_error_response(status: StatusCode, message: String) -> Response {
  Response::builder()
    .status(status)
    .body(message.into())
    .unwrap()
}

async fn prometheus_remote_write_handler(
  State(state): State<Arc<PromRemoteWriteInflow>>,
  ConnectInfo(addr): ConnectInfo<SocketAddr>,
  req: Request,
) -> Result<Response, StatusCode> {
  let received_at = Instant::now();
  state.stats.requests_total.inc();
  // TODO(mattklein123): Add bounds on max active requests.
  let _auto_request_active = AutoGauge::new(state.stats.requests_active.clone());

  // Length check to protect against malicious remote
  let req_content_length = req
    .body()
    .size_hint()
    .upper()
    .unwrap_or(MAX_ALLOWED_REQUEST_SIZE + 1);
  if req_content_length > MAX_ALLOWED_REQUEST_SIZE {
    state.stats.requests_4xx.inc();
    warn_every!(
      1.minutes(),
      "prometheus remote write request body size ({} bytes) exceeds limit ({} bytes)",
      req_content_length,
      MAX_ALLOWED_REQUEST_SIZE
    );
    return Ok(make_error_response(
      StatusCode::PAYLOAD_TOO_LARGE,
      format!(
        "body size ({req_content_length} bytes) exceeds limit ({MAX_ALLOWED_REQUEST_SIZE} bytes)"
      ),
    ));
  }

  // Pull the downstream ID depending on what is configured.
  let downstream_id_provider = DownstreamIdProviderImpl::new(
    state.config.append_tags_to_downstream_id,
    state.config.downstream_id_source.as_ref(),
    addr.ip(),
    &req,
  );
  let body = req
    .into_body()
    .collect()
    .await
    .map_err(|_| StatusCode::BAD_REQUEST)?
    .to_bytes();
  let write_request = match decode_body_into_write_request(&body) {
    Ok(wr) => wr,
    Err(e) => {
      state.stats.requests_4xx.inc();
      warn_every!(
        1.minutes(),
        "prometheus remote write request body failed to decode: {}",
        e
      );
      return Ok(make_error_response(
        StatusCode::BAD_REQUEST,
        format!("body failed to decode: {e}"),
      ));
    },
  };

  let (samples, errors) = ParsedMetric::from_write_request(
    write_request,
    received_at,
    &state.config.parse_config,
    &downstream_id_provider,
  );

  state.dispatcher.send(samples).await;

  // TODO(mattklein123): Integration test for prom inflow which makes sure we pass through metrics
  // after a failure.
  if errors.is_empty() {
    Ok(
      Response::builder()
        .status(StatusCode::OK)
        .body(().into())
        .unwrap(),
    )
  } else {
    state.stats.requests_4xx.inc();
    let errors = errors.into_iter().map(|e| e.to_string()).join(",");
    warn_every!(1.minutes(), "invalid write request: {}", errors);
    Ok(make_error_response(
      StatusCode::BAD_REQUEST,
      format!("invalid write request: {errors}"),
    ))
  }
}
