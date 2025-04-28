// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::InflowFactoryContext;
use crate::pipeline::PipelineDispatch;
use crate::protos::metric::{DownstreamId, DownstreamIdProvider, MetricId, ParsedMetric};
use axum::Router;
use axum::extract::{ConnectInfo, Request, State};
use axum::response::Response;
use axum::routing::{get, post};
use bd_grpc::axum_helper::serve_with_connect_info;
use bd_log::warn_every;
use bd_server_stats::stats::{AutoGauge, Scope};
use bd_shutdown::ComponentShutdownTriggerHandle;
use bytes::{BufMut, Bytes, BytesMut};
use http::{HeaderMap, StatusCode};
use http_body_util::BodyExt;
use hyper::body::Body;
use inflow_common::DownstreamIdSource;
use inflow_common::downstream_id_source::Source_type;
use log::info;
use parking_lot::Mutex;
use prometheus::{IntCounter, IntGauge};
use pulse_common::bind_resolver::BoundTcpSocket;
use pulse_protobuf::protos::pulse::config::inflow::v1::inflow_common::{self};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use time::ext::NumericalDuration;

const MAX_ALLOWED_REQUEST_SIZE: u64 = 20_000_000;

//
// Stats
//

#[derive(Clone)]
pub(super) struct Stats {
  connections_total: IntCounter,
  connections_active: IntGauge,
  requests_active: IntGauge,
  requests_total: IntCounter,
  pub(super) requests_4xx: IntCounter,
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
// HttpInflow
//

type RequestToResponse = dyn Fn(
    &HttpInflow,
    HeaderMap,
    Bytes,
    &dyn DownstreamIdProvider, // rustfmt issues
  ) -> (Vec<ParsedMetric>, Response)
  + Send
  + Sync;
pub(super) struct HttpInflow {
  pub(super) stats: Stats,
  bind: String,
  path: String,
  downstream_id_source: DownstreamIdSource,
  dispatcher: Arc<dyn PipelineDispatch>,
  shutdown_trigger_handle: ComponentShutdownTriggerHandle,
  socket: Mutex<Option<BoundTcpSocket>>,
  request_to_response: Box<RequestToResponse>,
}

impl HttpInflow {
  pub(crate) async fn new(
    bind: String,
    path: String,
    downstream_id_source: DownstreamIdSource,
    context: InflowFactoryContext,
    request_to_response: Box<RequestToResponse>,
  ) -> anyhow::Result<Self> {
    let stats = Stats::new(&context.scope);
    let socket = context.bind_resolver.resolve_tcp(&bind).await?;
    Ok(Self {
      stats,
      bind,
      path,
      downstream_id_source,
      dispatcher: context.dispatcher,
      shutdown_trigger_handle: context.shutdown_trigger_handle,
      socket: Mutex::new(Some(socket)),
      request_to_response,
    })
  }

  pub(crate) fn start(self: Arc<Self>) {
    let router = Router::new()
      .route("/healthcheck", get(|| async { "OK" }))
      .route(&self.path, post(remote_write_handler))
      .with_state(self.clone());

    let socket = self.socket.lock().take().unwrap();
    info!("remote write server starting at {}", socket.local_addr());
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
      info!("terminated remote write server running at {}", &self.bind);
    });
  }
}

//
// DecodeError
//

#[derive(thiserror::Error, Debug)]
pub(super) enum DecodeError {
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

pub(super) struct DownstreamIdProviderImpl {
  base: Bytes,
  append_tags_to_downstream_id: bool,
}

impl DownstreamIdProviderImpl {
  pub(super) fn new(
    downstream_id_source: &DownstreamIdSource,
    remote_addr: IpAddr,
    req: &Request,
  ) -> Self {
    // Note, there is currently no way to get the Bytes directly out of the header value so we have
    // to clone it.
    let base = downstream_id_source
      .source_type
      .as_ref()
      .and_then(|s| match s {
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
      append_tags_to_downstream_id: downstream_id_source.append_tags_to_downstream_id,
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

pub(super) fn make_error_response(status: StatusCode, message: String) -> Response {
  Response::builder()
    .status(status)
    .body(message.into())
    .unwrap()
}

async fn remote_write_handler(
  State(state): State<Arc<HttpInflow>>,
  ConnectInfo(addr): ConnectInfo<SocketAddr>,
  req: Request,
) -> Result<Response, StatusCode> {
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
      "remote write request body size ({} bytes) exceeds limit ({} bytes)",
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
  let downstream_id_provider =
    DownstreamIdProviderImpl::new(&state.downstream_id_source, addr.ip(), &req);
  let (parts, body) = req.into_parts();
  let body = body
    .collect()
    .await
    .map_err(|_| StatusCode::BAD_REQUEST)?
    .to_bytes();

  let (samples, response) =
    (state.request_to_response)(&state, parts.headers, body, &downstream_id_provider);
  state.dispatcher.send(samples).await;
  Ok(response)
}
