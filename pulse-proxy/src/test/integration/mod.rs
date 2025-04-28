// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::{ServerHooks, run_server};
use axum::extract::{Request, State};
use axum::handler::Handler;
use axum::response::Response;
use bd_proto::protos::prometheus::prompb::remote::WriteRequest;
use bd_server_stats::stats::Collector;
use bd_server_stats::test::util::stats::Helper as StatsHelper;
use bd_shutdown::ComponentShutdownTrigger;
use bd_test_helpers::make_mut;
use bytes::Bytes;
use fst::SetBuilder;
use futures::FutureExt;
use http::{HeaderMap, StatusCode};
use http_body_util::{BodyExt, Empty};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use parking_lot::Mutex;
use prom_remote_write::prom_remote_write_server_config::ParseConfig;
use protobuf::Message;
use pulse_common::bind_resolver::{
  BindResolver,
  BoundTcpSocket,
  make_reuse_port_tcp_socket,
  make_reuse_port_udp_socket,
};
use pulse_common::k8s::pods_info::PodsInfoSingleton;
use pulse_common::k8s::pods_info::container::PodsInfo;
use pulse_common::k8s::test::make_node_info;
use pulse_common::proto::yaml_to_proto;
use pulse_common::singleton::SingletonManager;
use pulse_metrics::clients::http::{
  HttpRemoteWriteClient,
  HyperHttpRemoteWriteClient,
  PROM_REMOTE_WRITE_HEADERS,
};
use pulse_metrics::pipeline::MockPipelineDispatch;
use pulse_metrics::pipeline::inflow::wire::tcp::TcpInflow;
use pulse_metrics::pipeline::inflow::{InflowFactoryContext, PipelineInflow};
use pulse_metrics::pipeline::outflow::prom::compress_write_request;
use pulse_metrics::protos::metric::ParsedMetric;
use pulse_metrics::protos::prom::{
  ChangedTypeTracker,
  MetadataType,
  ToWriteRequestOptions,
  to_write_request,
};
use pulse_metrics::test::{TestDownstreamIdProvider, make_carbon_wire_protocol};
use pulse_protobuf::protos::pulse::config::bootstrap::v1::bootstrap::KubernetesBootstrapConfig;
use pulse_protobuf::protos::pulse::config::inflow::v1::prom_remote_write;
use pulse_protobuf::protos::pulse::config::inflow::v1::wire::TcpServerConfig;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use time::ext::{NumericalDuration, NumericalStdDuration};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::{mpsc, watch};
use tokio::time::timeout;

mod aggregation;
mod blocklisting;
mod complex_config;
mod dynamic_fs_config;
mod meta_stats;
mod multi_node;
mod prom;
mod single_node;
mod socket_close;
mod statsd;

pub fn current_time() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_secs()
}

async fn write_all(stream: &mut TcpStream, lines: &[&str]) {
  for line in lines {
    stream.write_all(line.as_bytes()).await.unwrap();
  }
}

async fn make_admin_request(addr: SocketAddr, path: &str) -> String {
  let client = Client::builder(TokioExecutor::new()).build_http::<Empty<Bytes>>();
  let response = client
    .get(format!("http://{addr}{path}",).try_into().unwrap())
    .await
    .unwrap();
  assert!(response.status().is_success());
  String::from_utf8(
    response
      .into_body()
      .collect()
      .await
      .unwrap()
      .to_bytes()
      .to_vec(),
  )
  .unwrap()
}

//
// HelperHooks
//

// Test startup hooks.
struct HelperHooks {
  tx: mpsc::Sender<Collector>,
}

#[async_trait::async_trait]
impl ServerHooks for HelperHooks {
  async fn server_started(&self, collector: Collector) {
    self.tx.send(collector).await.unwrap();
  }
}

//
// ResolvedListener
//

//
// HelperBindResolver
//

// Test implementation of the BindResolver which allows for registering a number of sockets bound
// to port 0, which can then be returned later.
struct HelperBindResolver {
  tcp_sockets: Mutex<HashMap<String, BoundTcpSocket>>,
  udp_sockets: Mutex<HashMap<String, UdpSocket>>,
}

impl HelperBindResolver {
  async fn new(tcp_names: &[&str], udp_names: &[&str]) -> Arc<Self> {
    let mut tcp_sockets = HashMap::new();
    for name in tcp_names {
      tcp_sockets.insert(
        (*name).to_string(),
        make_reuse_port_tcp_socket("127.0.0.1:0").await.unwrap(),
      );
    }
    let mut udp_sockets = HashMap::new();
    for name in udp_names {
      udp_sockets.insert(
        (*name).to_string(),
        make_reuse_port_udp_socket("127.0.0.1:0").await.unwrap(),
      );
    }

    Arc::new(Self {
      tcp_sockets: Mutex::new(tcp_sockets),
      udp_sockets: Mutex::new(udp_sockets),
    })
  }

  fn local_tcp_addr(&self, name: &str) -> SocketAddr {
    self
      .tcp_sockets
      .lock()
      .get(name)
      .unwrap_or_else(|| {
        panic!("bind resolver could not find TCP '{name}'. Make sure it is registered.",)
      })
      .local_addr()
  }

  // Note that this removes the socket so that it can be closed. For UDP with reuse port the kernel
  // can distribute packets to this socket so for tests we want to force packets to go to the
  // socket being used by the server.
  fn take_udp_addr(&self, name: &str) -> SocketAddr {
    self
      .udp_sockets
      .lock()
      .remove(name)
      .unwrap_or_else(|| {
        panic!("bind resolver could not find UDP '{name}'. Make sure it is registered.",)
      })
      .local_addr()
      .unwrap()
  }
}

#[async_trait::async_trait]
impl BindResolver for HelperBindResolver {
  async fn resolve_tcp(&self, name: &str) -> anyhow::Result<BoundTcpSocket> {
    let local_addr = {
      let mut tcp_sockets = self.tcp_sockets.lock();
      let socket = tcp_sockets
        .get_mut(name)
        .unwrap_or_else(|| panic!("could not resolve '{name}'. Make sure it is registered."));
      socket.local_addr().to_string()
    };
    Ok(make_reuse_port_tcp_socket(&local_addr).await.unwrap())
  }

  async fn resolve_udp(&self, name: &str) -> anyhow::Result<UdpSocket> {
    let local_addr = {
      let mut udp_sockets = self.udp_sockets.lock();
      let socket = udp_sockets
        .get_mut(name)
        .unwrap_or_else(|| panic!("could not resolve '{name}'. Make sure it is registered."));
      socket.local_addr().unwrap().to_string()
    };
    Ok(make_reuse_port_udp_socket(&local_addr).await.unwrap())
  }
}

//
// Helper
//

// Integration test helper for running the server.
pub struct Helper {
  shutdown_trigger: Option<ComponentShutdownTrigger>,
  shutdown: bool,
  stats: StatsHelper,
}

impl Drop for Helper {
  fn drop(&mut self) {
    assert!(self.shutdown, "call shutdown() at the end of the test");
  }
}

impl Helper {
  pub async fn new(config_yaml: &str, bind_resolver: Arc<dyn BindResolver>) -> Self {
    Self::new_with_k8s(config_yaml, bind_resolver, None).await
  }

  pub async fn new_with_k8s(
    config_yaml: &str,
    bind_resolver: Arc<dyn BindResolver>,
    k8s_rx: Option<watch::Receiver<PodsInfo>>,
  ) -> Self {
    let config = yaml_to_proto(config_yaml).unwrap();
    let shutdown_trigger = ComponentShutdownTrigger::default();
    let mut shutdown = shutdown_trigger.make_shutdown();
    let (tx, mut rx) = mpsc::channel(1);
    tokio::spawn(async move {
      run_server(
        config,
        false,
        || shutdown.cancelled(),
        Duration::ZERO,
        HelperHooks { tx },
        bind_resolver,
        Arc::default(),
        move |_| {
          let cloned_k8s_rx = k8s_rx.clone().expect("k8s watch not configured");
          async move {
            Ok(Arc::new(PodsInfoSingleton::new(
              cloned_k8s_rx,
              Arc::new(make_node_info()),
            )))
          }
        },
      )
      .await
    });
    let collector = rx.recv().await.unwrap();
    Self {
      shutdown_trigger: Some(shutdown_trigger),
      shutdown: false,
      stats: StatsHelper::new_with_collector(collector),
    }
  }

  pub async fn shutdown(mut self) {
    self.shutdown_trigger.take().unwrap().shutdown().await;
    self.shutdown = true;
  }

  pub const fn stats_helper(&self) -> &StatsHelper {
    &self.stats
  }
}

//
// FakeRemoteFileSource
//

#[derive(Default)]
struct FakeRemoteFileSource {
  files: Mutex<HashMap<String, (Bytes, String)>>,
}

impl FakeRemoteFileSource {
  #[allow(clippy::unused_async)]
  async fn handler(
    State(state): State<Arc<Self>>,
    request: Request,
  ) -> Result<Response, StatusCode> {
    log::debug!("request for: {}", request.uri().path());
    state.files.lock().get(request.uri().path()).map_or(
      Err(StatusCode::NOT_FOUND),
      |(bytes, etag)| {
        if request
          .headers()
          .get("if-none-match")
          .is_some_and(|v| v.as_bytes() == etag.as_bytes())
        {
          return Ok(
            Response::builder()
              .status(StatusCode::NOT_MODIFIED)
              .body(().into())
              .unwrap(),
          );
        }

        Ok(
          Response::builder()
            .header("etag", etag)
            .body(bytes.clone().into())
            .unwrap(),
        )
      },
    )
  }

  pub fn update_file(&self, path: &str, file: Bytes) {
    let hash = ahash::RandomState::with_seeds(0, 0, 0, 0)
      .hash_one(&file)
      .to_string();
    self.files.lock().insert(path.to_string(), (file, hash));
  }

  pub fn remove_file(&self, path: &str) {
    self.files.lock().remove(path);
  }

  pub async fn new(name: &str, bind_resolver: Arc<dyn BindResolver>) -> Arc<Self> {
    let fake_remote_file_source = Arc::new(Self::default());
    let handler = Self::handler.with_state(fake_remote_file_source.clone());
    let server = axum::serve(
      bind_resolver.resolve_tcp(name).await.unwrap().listen(),
      handler.into_make_service(),
    );
    tokio::spawn(async move {
      server.await.unwrap();
    });

    fake_remote_file_source
  }

  pub fn make_fst(names: &[&str]) -> Bytes {
    let mut names = names.to_vec();
    names.sort_unstable();
    let mut builder = SetBuilder::memory();
    for name in names {
      builder.insert(name).unwrap();
    }
    builder.into_inner().unwrap().into()
  }

  pub fn make_regex_csv(regex: &[&str]) -> Bytes {
    regex.join("\n").into()
  }
}

//
// FakePromUpstream
//

type FakePromData = (HeaderMap, Vec<ParsedMetric>);
struct FakePromUpstreamState {
  tx: mpsc::Sender<FakePromData>,
  failure_response_codes: Mutex<Vec<StatusCode>>,
  parse_config: ParseConfig,
}
struct FakePromUpstream {
  rx: mpsc::Receiver<FakePromData>,
  state: Arc<FakePromUpstreamState>,
}

impl FakePromUpstream {
  async fn handler(
    State(state): State<Arc<FakePromUpstreamState>>,
    request: Request,
  ) -> Result<Response, StatusCode> {
    if let Some(status_code) = state.failure_response_codes.lock().pop() {
      return Err(status_code);
    }

    let (parts, body) = request.into_parts();
    let body = body.collect().await.unwrap().to_bytes();
    let decompressed = snap::raw::Decoder::new().decompress_vec(&body).unwrap();
    let write_request = WriteRequest::parse_from_tokio_bytes(&Bytes::from(decompressed)).unwrap();
    let (mut metrics, errors) = ParsedMetric::from_write_request(
      write_request,
      Instant::now(),
      &state.parse_config,
      &TestDownstreamIdProvider {},
    );

    // Sort metrics by name so that we can have consistent matching.
    metrics.sort_by(|lhs, rhs| {
      lhs
        .metric()
        .get_id()
        .name()
        .cmp(rhs.metric().get_id().name())
    });

    assert!(errors.is_empty(), "{errors:?}");
    state.tx.send((parts.headers, metrics)).await.unwrap();

    Ok(
      Response::builder()
        .status(StatusCode::OK)
        .body(().into())
        .unwrap(),
    )
  }

  pub fn add_failure_response_code(&self, status_code: StatusCode) {
    self.state.failure_response_codes.lock().push(status_code);
  }

  pub async fn new(
    name: &str,
    bind_resolver: Arc<dyn BindResolver>,
    parse_config: ParseConfig,
  ) -> Self {
    let (tx, rx) = mpsc::channel(128);
    let state = Arc::new(FakePromUpstreamState {
      tx,
      failure_response_codes: Mutex::default(),
      parse_config,
    });
    let handler = Self::handler.with_state(state.clone());
    let server = axum::serve(
      bind_resolver.resolve_tcp(name).await.unwrap().listen(),
      handler.into_make_service(),
    );
    tokio::spawn(async move {
      server.await.unwrap();
    });

    Self { rx, state }
  }

  async fn wait_for_metrics(&mut self) -> (HeaderMap, Vec<ParsedMetric>) {
    self.rx.recv().await.unwrap()
  }
}

//
// FakeWireUpstream
//

// Fake upstream that expects wire protocol stats.
struct FakeWireUpstream {
  inflow: Arc<TcpInflow>,
  rx: mpsc::Receiver<Vec<ParsedMetric>>,
  _shutdown_trigger: ComponentShutdownTrigger,
}

impl FakeWireUpstream {
  pub async fn new(name: &str, bind_resolver: Arc<dyn BindResolver>) -> Self {
    let mut dispatcher = Arc::new(MockPipelineDispatch::new());
    let (tx, rx) = mpsc::channel(128);
    make_mut(&mut dispatcher)
      .expect_send()
      .returning(move |samples| {
        tx.try_send(samples).unwrap();
      });

    let shutdown_trigger = ComponentShutdownTrigger::default();
    let upstream = Self {
      inflow: Arc::new(
        TcpInflow::new(
          TcpServerConfig {
            bind: name.to_string().into(),
            protocol: Some(make_carbon_wire_protocol()).into(),
            ..Default::default()
          },
          InflowFactoryContext {
            name: name.to_string(),
            scope: Collector::default().scope("fake_upstream").scope(name),
            dispatcher,
            shutdown_trigger_handle: shutdown_trigger.make_handle(),
            bind_resolver,
            k8s_config: KubernetesBootstrapConfig::default(),
            k8s_watch_factory: Arc::new(|| async { unreachable!() }.boxed()),
            singleton_manager: Arc::new(SingletonManager::default()),
          },
        )
        .await
        .unwrap(),
      ),
      rx,
      _shutdown_trigger: shutdown_trigger,
    };
    upstream.inflow.clone().start().await;
    upstream
  }

  // Wait for a specific number metrics before returning. Each batch has a 10s timeout.
  pub async fn wait_for_num_metrics(&mut self, num: usize) -> Vec<ParsedMetric> {
    let mut metrics = Vec::new();
    loop {
      metrics.extend(self.wait_for_metrics().await);
      if metrics.len() >= num {
        return metrics;
      }
    }
  }

  // Wait for metrics with a 10s timeout.
  pub async fn wait_for_metrics(&mut self) -> Vec<ParsedMetric> {
    timeout(10.std_seconds(), self.rx.recv())
      .await
      .unwrap()
      .unwrap()
  }
}

//
// PromClient
//

// Helper for making prom remote write requests.
struct PromClient {
  client: HyperHttpRemoteWriteClient,
}

impl PromClient {
  async fn new(addr: SocketAddr) -> Self {
    Self {
      client: HyperHttpRemoteWriteClient::new(
        format!("http://{addr}/api/v1/prom/write"),
        1.seconds(),
        None,
        PROM_REMOTE_WRITE_HEADERS,
        vec![],
      )
      .await
      .unwrap(),
    }
  }

  async fn send(&self, metrics: Vec<ParsedMetric>) {
    let write_request = to_write_request(
      metrics,
      &ToWriteRequestOptions {
        metadata: MetadataType::Normal,
        convert_name: false,
      },
      &ChangedTypeTracker::new_for_test(),
    );
    self
      .client
      .send_write_request(compress_write_request(&write_request).into(), None)
      .await
      .unwrap();
  }
}
