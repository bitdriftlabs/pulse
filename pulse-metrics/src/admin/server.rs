// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::stats::StatsProvider;
use crate::clients::client::ConnectTo;
use crate::clients::client_pool;
use crate::clients::client_pool::Pool;
use crate::clients::http::{
  HttpRemoteWriteClient,
  HyperHttpRemoteWriteClient,
  PROM_REMOTE_WRITE_HEADERS,
  should_retry,
};
use crate::pipeline::config::DEFAULT_REQUEST_TIMEOUT;
use crate::pipeline::outflow::prom::compress_write_request;
use crate::pipeline::time::{RealTimeProvider, next_flush_interval};
use anyhow::bail;
use async_trait::async_trait;
use axum::Router;
use axum::extract::{Query, Request, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::{get, post};
use backoff::ExponentialBackoffBuilder;
use backoff::future::retry;
use bd_log::SwapLogger;
use bd_server_stats::stats::Collector;
use bd_shutdown::ComponentShutdown;
use bd_time::TimeDurationExt;
use bytes::Bytes;
use config::bootstrap::v1::bootstrap::meta_stats::MetaProtocol;
use config::bootstrap::v1::bootstrap::meta_stats::meta_protocol::Protocol;
use config::common::v1::common::WireProtocol;
use config::common::v1::common::wire_protocol::Protocol_type;
use futures::Future;
use log::{info, warn};
use parking_lot::RwLock;
use pulse_common::bind_resolver::BindResolver;
use pulse_common::proto::ProtoDurationToStdDuration;
use pulse_protobuf::protos::pulse::config;
use std::boxed::Box;
use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::pin::Pin;
use std::sync::Arc;
use std::time::SystemTime;
use time::Duration;
use time::ext::NumericalStdDuration;

//
// Admin
//

// Trait for the server's admin functionality.
pub type AdminHandler =
  Box<dyn Fn(Request) -> Pin<Box<dyn Future<Output = Response> + Send>> + Send + Sync>;
pub trait AdminHandlerHandle: Send + Sync {}
pub trait Admin: Send + Sync {
  // Register a custom handler at a specific path.
  fn register_handler(
    &self,
    path: &str,
    handler: AdminHandler,
  ) -> anyhow::Result<Box<dyn AdminHandlerHandle>>;
}

//
// AdminHandlerHandleImpl
//

struct AdminHandlerHandleImpl {
  path: String,
  extra_handlers: Arc<RwLock<HashMap<String, AdminHandler>>>,
}

impl AdminHandlerHandle for AdminHandlerHandleImpl {}

impl Drop for AdminHandlerHandleImpl {
  fn drop(&mut self) {
    debug_assert!(self.extra_handlers.read().contains_key(&self.path));
    self.extra_handlers.write().remove(&self.path);
    log::info!("unregistered {} admin handler", self.path);
  }
}

//
// AdminState
//

// Real implementation of the admin server.
pub struct AdminState {
  collector: Collector,
  extra_handlers: Arc<RwLock<HashMap<String, AdminHandler>>>,
}

impl Drop for AdminState {
  fn drop(&mut self) {
    debug_assert!(self.extra_handlers.read().is_empty());
  }
}

impl Admin for AdminState {
  fn register_handler(
    &self,
    path: &str,
    handler: AdminHandler,
  ) -> anyhow::Result<Box<dyn AdminHandlerHandle>> {
    let mut extra_handlers = self.extra_handlers.write();
    if extra_handlers.contains_key(path) {
      bail!("admin handler '{path}' already registered");
    }

    extra_handlers.insert(path.to_string(), handler);
    log::info!("registered {path} admin handler");
    Ok(Box::new(AdminHandlerHandleImpl {
      path: path.to_string(),
      extra_handlers: self.extra_handlers.clone(),
    }))
  }
}

impl AdminState {
  #[must_use]
  pub fn new(collector: Collector) -> Arc<Self> {
    Arc::new(Self {
      collector,
      extra_handlers: Arc::default(),
    })
  }

  #[allow(clippy::unused_async)]
  async fn metrics(State(state): State<Arc<Self>>) -> Response {
    let buffer = state.collector.prometheus_output();
    Response::builder()
      .header(axum::http::header::CONTENT_TYPE, prometheus::TEXT_FORMAT)
      .body(buffer.into())
      .unwrap()
  }

  fn result_to_string(result: tikv_jemalloc_ctl::Result<()>) -> String {
    result.map_or_else(|e| format!("error: {e}"), |()| "OK".to_string())
  }

  #[allow(clippy::unused_async)]
  async fn profile_enable() -> String {
    unsafe { Self::result_to_string(tikv_jemalloc_ctl::raw::write(b"prof.active\0", true)) }
  }

  #[allow(clippy::unused_async)]
  async fn profile_disable() -> String {
    unsafe { Self::result_to_string(tikv_jemalloc_ctl::raw::write(b"prof.active\0", false)) }
  }

  #[allow(clippy::unused_async)]
  async fn profile_dump() -> String {
    use std::ffi::CString;

    let output_file = CString::new("/tmp/profile.heap").unwrap();

    unsafe {
      Self::result_to_string(tikv_jemalloc_ctl::raw::write_str(
        b"prof.dump\0",
        std::mem::transmute::<&[u8], &[u8]>(output_file.as_bytes_with_nul()),
      ))
    }
  }

  #[allow(clippy::unused_async)]
  async fn root() -> String {
    "pulse-proxy admin server".to_string()
  }

  #[allow(clippy::unused_async)]
  async fn healthcheck() -> String {
    "OK".to_string()
  }

  #[allow(clippy::unused_async)]
  async fn log_filter(Query(mut params): Query<HashMap<String, String>>) -> String {
    let Some(filter) = params.remove("filter") else {
      return "usage: /log_filter?filter=RUST_LOG".to_string();
    };
    log::info!("updating log filter: {filter}");
    if let Err(e) = SwapLogger::swap(&filter) {
      log::warn!("error updating log filter: {e}");
    }

    "OK".to_string()
  }

  async fn fallback_handler(State(state): State<Arc<Self>>, request: Request) -> Response {
    let handler = state
      .extra_handlers
      .read()
      .get(request.uri().path())
      .map(|h| h(request));
    if let Some(handler) = handler {
      handler.await
    } else {
      Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body("not found".into())
        .unwrap()
    }
  }

  fn make_router(self: Arc<Self>) -> Router {
    Router::new()
      .route("/", get(Self::root))
      .route("/healthcheck", get(Self::healthcheck))
      .route("/log_filter", post(Self::log_filter))
      .route("/metrics", get(Self::metrics))
      .route("/profile_enable", post(Self::profile_enable))
      .route("/profile_disable", post(Self::profile_disable))
      .route("/profile_dump", post(Self::profile_dump))
      .fallback(Self::fallback_handler)
      .with_state(self)
  }

  pub async fn spawn_server(
    self: Arc<Self>,
    bind_resolver: Arc<dyn BindResolver>,
    bind: &str,
  ) -> anyhow::Result<()> {
    let router = self.make_router();
    let socket = bind_resolver.resolve_tcp(bind).await?;
    info!("admin server starting on: {}", socket.local_addr());
    axum::serve(socket.listen(), router).await?;
    Ok(())
  }
}

#[async_trait]
trait MetaStatsSender: Send + Sync {
  async fn emit(&self, stats_provider: &StatsProvider) -> Result<(), Error>;
}

struct WireMetaStatsSender {
  protocol: WireProtocol,
  pool: Pool,
}

#[async_trait]
impl MetaStatsSender for WireMetaStatsSender {
  async fn emit(&self, stats_provider: &StatsProvider) -> Result<(), Error> {
    let output = match self.protocol.protocol_type {
      Some(Protocol_type::Carbon(_)) => {
        let now = SystemTime::now()
          .duration_since(SystemTime::UNIX_EPOCH)
          .unwrap()
          .as_secs();
        stats_provider.carbon_output(now)
      },
      Some(Protocol_type::Statsd(_)) => stats_provider.statsd_output(),
      None => unreachable!("pgv"),
    };

    let mut client = self
      .pool
      .get()
      .await
      .map_err(|e| Error::new(ErrorKind::ConnectionRefused, e.to_string()))?;
    client.write(&mut output.as_slice()).await
  }
}

struct PromRemoteWriteMetaStatsSender {
  client: HyperHttpRemoteWriteClient,
}

#[async_trait]
impl MetaStatsSender for PromRemoteWriteMetaStatsSender {
  async fn emit(&self, stats_provider: &StatsProvider) -> Result<(), Error> {
    // TODO(mattklein123): We need to adhere to max batch size here as we can go above request
    // size limits. Assuming we are failing for size reasons we need to put a batch between the
    // output and the sending, similar to the outflow (so perhaps we should just use the outflow).
    let compressed_write_request: Bytes =
      compress_write_request(&stats_provider.prometheus_remote_write_output()).into();
    retry(
      // Arbitrary, set to 1 minute to match the flush interval.
      ExponentialBackoffBuilder::new()
        .with_max_elapsed_time(Some(1.std_minutes()))
        .build(),
      || async {
        match self
          .client
          .send_write_request(compressed_write_request.clone(), None)
          .await
        {
          Ok(()) => Ok(()),
          Err(e) => {
            let translated_e = Error::new(
              ErrorKind::Other,
              format!(
                "request (size={}) failed: {}",
                compressed_write_request.len(),
                e
              ),
            );
            if should_retry(&e) {
              Err(backoff::Error::transient(translated_e))
            } else {
              Err(backoff::Error::permanent(translated_e))
            }
          },
        }
      },
    )
    .await
  }
}

pub struct MetaStatsEmitter {
  shutdown: ComponentShutdown,
  stats_provider: StatsProvider,
  senders: Vec<Box<dyn MetaStatsSender>>,
  flush_interval: Duration,
}

impl MetaStatsEmitter {
  pub async fn new(
    shutdown: ComponentShutdown,
    stats_provider: StatsProvider,
    protocols: Vec<MetaProtocol>,
    flush_interval: Duration,
  ) -> anyhow::Result<Self> {
    let mut senders = Vec::new();
    for protocol in protocols {
      let sender: Box<dyn MetaStatsSender> = match protocol.protocol.expect("pgv") {
        Protocol::Wire(wire) => Box::new(WireMetaStatsSender {
          protocol: wire.common.protocol.clone().unwrap(),
          pool: client_pool::new(
            ConnectTo::TcpSocketAddr(wire.common.send_to.to_string()),
            None,
          ),
        }) as Box<dyn MetaStatsSender>,
        Protocol::PromRemoteWrite(prom_remote_write) => Box::new(PromRemoteWriteMetaStatsSender {
          client: HyperHttpRemoteWriteClient::new(
            prom_remote_write.send_to.into(),
            prom_remote_write
              .request_timeout
              .unwrap_duration_or(DEFAULT_REQUEST_TIMEOUT),
            prom_remote_write.auth.into_option(),
            PROM_REMOTE_WRITE_HEADERS,
            prom_remote_write.request_headers,
          )
          .await?,
        }) as Box<dyn MetaStatsSender>,
      };

      senders.push(sender);
    }

    Ok(Self {
      shutdown,
      stats_provider,
      senders,
      flush_interval,
    })
  }

  async fn send_meta_stats(&self) {
    let results = futures::future::join_all(
      self
        .senders
        .iter()
        .map(|sender| sender.emit(&self.stats_provider)),
    )
    .await;

    for result in results {
      if let Err(e) = result {
        warn!("error writing meta stats to sink due to: {e}");
      }
    }
  }

  pub async fn run(mut self) {
    loop {
      tokio::select! {
        () = self.shutdown.cancelled() => break,
        () = next_flush_interval(
          &RealTimeProvider{}, self.flush_interval.whole_seconds()).sleep() => {
          self.send_meta_stats().await;
        }
      }
    }

    // Do a final flush at shutdown.
    self.send_meta_stats().await;
  }
}
