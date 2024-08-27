// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::{Scraper, Stats, Ticker};
use crate::pipeline::inflow::prom_scrape::scraper::{EndpointProvider, KubePodTarget};
use crate::pipeline::time::TestDurationJitter;
use crate::pipeline::MockPipelineDispatch;
use axum::async_trait;
use axum::body::Body;
use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use bd_shutdown::ComponentShutdownTrigger;
use http::StatusCode;
use parking_lot::Mutex;
use prometheus::labels;
use pulse_common::k8s::pods_info::container::PodsInfo;
use pulse_common::k8s::pods_info::{PodsInfoSingleton, PromEndpoint};
use pulse_common::k8s::test::make_pod_info;
use pulse_common::metadata::Metadata;
use std::collections::{HashMap, VecDeque};
use std::future::pending;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use time::ext::NumericalDuration;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use vrl::{btreemap, path};

#[tokio::test]
async fn test_kube_pod_target_endpoint() {
  let mut initial_state = PodsInfo::default();
  initial_state.insert(make_pod_info(
    "some-namespace",
    "my-awesome-pod",
    &btreemap!(),
    btreemap!(),
    Some(PromEndpoint::new(
      "127.0.0.1".parse().unwrap(),
      9090,
      "/metrics",
      Some(Arc::new(Metadata::new(
        "some-namespace",
        "my-awesome-pod",
        &btreemap!(),
        &btreemap!(),
        Some("some-service"),
      ))),
    )),
    HashMap::new(),
    "127.0.0.1",
  ));

  let (_tx, rx) = tokio::sync::watch::channel(initial_state);
  let pods_info = Arc::new(PodsInfoSingleton::new(rx)).make_owned();
  let mut target = KubePodTarget { pods_info };
  let endpoints = target.get();
  assert_eq!(endpoints.len(), 1);

  assert_eq!(
    endpoints["some-namespace/my-awesome-pod"]
      .metadata
      .as_ref()
      .unwrap()
      .value()
      .get(path!("k8s", "namespace"))
      .unwrap()
      .as_str()
      .unwrap(),
    "some_namespace"
  );
  assert_eq!(
    endpoints["some-namespace/my-awesome-pod"]
      .metadata
      .as_ref()
      .unwrap()
      .value()
      .get(path!("k8s", "pod", "name"))
      .unwrap()
      .as_str()
      .unwrap(),
    "my_awesome_pod"
  );
  assert_eq!(
    endpoints["some-namespace/my-awesome-pod"]
      .metadata
      .as_ref()
      .unwrap()
      .value()
      .get(path!("k8s", "service", "name"))
      .unwrap()
      .as_str()
      .unwrap(),
    "some_service"
  );
}

#[tokio::test]
async fn test_unavailable() {
  let mut setup = Setup::new(false, false, false).await;

  setup.tick_tx.send(()).await.unwrap();

  setup
    .stats_helper
    .wait_for_counter_eq(1, "test:scrape_failure", &labels! {})
    .await;

  setup.tick_tx.send(()).await.unwrap();

  setup
    .stats_helper
    .wait_for_counter_eq(2, "test:scrape_failure", &labels! {})
    .await;

  setup.shutdown().await;
}

#[tokio::test]
async fn test_invalid_status_code() {
  let mut setup = Setup::new(true, true, false).await;

  setup.tick_tx.send(()).await.unwrap();

  setup
    .stats_helper
    .wait_for_counter_eq(1, "test:scrape_attempt", &labels! {})
    .await;
  setup
    .stats_helper
    .wait_for_counter_eq(1, "test:scrape_failure", &labels! {})
    .await;

  assert_eq!(setup.server.1.load(Ordering::SeqCst), 1);

  setup.tick_tx.send(()).await.unwrap();

  setup
    .stats_helper
    .wait_for_counter_eq(2, "test:scrape_attempt", &labels! {})
    .await;
  setup
    .stats_helper
    .wait_for_counter_eq(2, "test:scrape_failure", &labels! {})
    .await;

  assert_eq!(setup.server.1.load(Ordering::SeqCst), 2);

  setup.shutdown().await;
}

#[tokio::test]
async fn test_invalid_body() {
  let mut setup = Setup::new(true, false, true).await;

  setup.tick_tx.send(()).await.unwrap();

  setup
    .stats_helper
    .wait_for_counter_eq(1, "test:scrape_attempt", &labels! {})
    .await;
  setup
    .stats_helper
    .wait_for_counter_eq(1, "test:parse_failure", &labels! {})
    .await;

  assert_eq!(setup.server.1.load(Ordering::SeqCst), 1);

  setup.tick_tx.send(()).await.unwrap();

  setup
    .stats_helper
    .wait_for_counter_eq(2, "test:scrape_attempt", &labels! {})
    .await;
  setup
    .stats_helper
    .wait_for_counter_eq(2, "test:parse_failure", &labels! {})
    .await;

  assert_eq!(setup.server.1.load(Ordering::SeqCst), 2);

  setup.shutdown().await;
}

#[tokio::test]
async fn test_calls() {
  let mut setup = Setup::new(true, false, false).await;

  setup.tick_tx.send(()).await.unwrap();

  setup
    .stats_helper
    .wait_for_counter_eq(1, "test:scrape_complete", &labels! {})
    .await;

  assert_eq!(setup.server.1.load(Ordering::SeqCst), 1);

  setup.tick_tx.send(()).await.unwrap();

  setup
    .stats_helper
    .wait_for_counter_eq(2, "test:scrape_complete", &labels! {})
    .await;

  assert_eq!(setup.server.1.load(Ordering::SeqCst), 2);

  setup.shutdown().await;

  assert!(setup.tick_tx.send(()).await.is_err());
}

//
// FakeTicker
//

struct FakeTicker(mpsc::Receiver<()>);

#[async_trait]
impl Ticker for FakeTicker {
  async fn next(&mut self) {
    let _ignored = self.0.recv().await;
  }
}

//
// FakeTickerFactory
//

#[derive(Default)]
struct FakeTickerFactory {
  rx_list: Mutex<VecDeque<mpsc::Receiver<()>>>,
}

impl FakeTickerFactory {
  fn add_rx(&self, rx: mpsc::Receiver<()>) {
    self.rx_list.lock().push_back(rx);
  }

  fn make_ticker(&self) -> Box<dyn Ticker> {
    let rx = self.rx_list.lock().pop_front().unwrap();
    Box::new(FakeTicker(rx))
  }
}

//
// FakeEndpointProvider
//

struct FakeEndpointProvider {
  target_server: bool,
  port: u16,
}

#[async_trait]
impl EndpointProvider for FakeEndpointProvider {
  fn get(&mut self) -> HashMap<String, PromEndpoint> {
    [(
      "foo".to_string(),
      PromEndpoint {
        url: format!(
          "http://localhost:{}",
          if self.target_server { self.port } else { 1234 }
        ),
        metadata: Some(Arc::new(Metadata::new(
          "namespace",
          "pod",
          &btreemap!(),
          &btreemap!(),
          None,
        ))),
      },
    )]
    .into()
  }

  async fn changed(&mut self) {
    pending::<()>().await;
  }
}

//
// Setup
//

struct Setup {
  stats_helper: bd_server_stats::test::util::stats::Helper,
  tick_tx: mpsc::Sender<()>,
  shutdown_trigger: Option<ComponentShutdownTrigger>,
  server: (u16, Arc<AtomicU64>),
  shutdown_called: bool,
}

impl Setup {
  async fn new(target_server: bool, invalid_status_code: bool, invalid_body: bool) -> Self {
    let stats_helper = bd_server_stats::test::util::stats::Helper::new();
    let stats = Stats::new(&stats_helper.collector().scope("test"));
    let (tick_tx, tick_rx) = tokio::sync::mpsc::channel(1);
    let ticker_factory = Arc::new(FakeTickerFactory::default());
    ticker_factory.add_rx(tick_rx);

    let server = TestPromServer::start(
      if invalid_status_code {
        StatusCode::BAD_GATEWAY
      } else {
        StatusCode::OK
      },
      if invalid_body {
        Some("not a prom response".to_string())
      } else {
        None
      },
    )
    .await;

    let shutdown_trigger = ComponentShutdownTrigger::default();
    let scraper = Scraper::<_, TestDurationJitter>::create(
      "test".to_string(),
      stats,
      Arc::new(MockPipelineDispatch::new()),
      shutdown_trigger.make_handle(),
      FakeEndpointProvider {
        target_server,
        port: server.0,
      },
      false,
      0.seconds(),
      Box::new(move || ticker_factory.make_ticker()),
    )
    .unwrap();
    scraper.start().await;

    Self {
      stats_helper,
      tick_tx,
      server,
      shutdown_trigger: Some(shutdown_trigger),
      shutdown_called: false,
    }
  }

  async fn shutdown(&mut self) {
    self.shutdown_trigger.take().unwrap().shutdown().await;
    self.shutdown_called = true;
  }
}

impl Drop for Setup {
  fn drop(&mut self) {
    assert!(self.shutdown_called);
  }
}

//
// TestPromServer
//

struct TestPromServer {
  calls: Arc<AtomicU64>,
  response_code: axum::http::StatusCode,
  body: Option<String>,
}

impl TestPromServer {
  async fn start(
    response_code: axum::http::StatusCode,
    body: Option<String>,
  ) -> (u16, Arc<AtomicU64>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();

    let port = listener.local_addr().unwrap().port();

    let calls = Arc::new(AtomicU64::new(0));
    let server = Arc::new(Self {
      calls: calls.clone(),
      response_code,
      body,
    });

    tokio::spawn(async move {
      axum::serve(listener, server.router().into_make_service())
        .await
        .unwrap();
    });

    (port, calls)
  }

  fn router(self: Arc<Self>) -> axum::Router {
    axum::Router::new()
      .route("/", get(metrics))
      .with_state(self)
  }
}

async fn metrics(State(server): State<Arc<TestPromServer>>) -> Response {
  server.calls.fetch_add(1, Ordering::SeqCst);

  let body = server.body.clone();

  let body = body.map_or_else(Body::empty, std::convert::Into::into);

  let mut response = Response::new(body);
  *response.status_mut() = server.response_code;
  response
}
