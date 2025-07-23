// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::{PromEndpoint, Scraper, Stats, Ticker};
use crate::pipeline::MockPipelineDispatch;
use crate::pipeline::inflow::prom_scrape::http_sd::TargetBlock;
use crate::pipeline::inflow::prom_scrape::scraper::{
  EndpointProvider,
  HttpServiceDiscoveryEndpointTarget,
  KubeEndpointsTarget,
  KubePodTarget,
  create_endpoints,
};
use crate::pipeline::time::TestDurationJitter;
use crate::protos::metric::TagValue;
use async_trait::async_trait;
use axum::body::Body;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use bd_shutdown::{ComponentShutdown, ComponentShutdownTrigger};
use bd_test_helpers::{float_eq, make_mut};
use futures::FutureExt;
use http::StatusCode;
use itertools::Itertools;
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use k8s_prom::kubernetes_prometheus_config::inclusion_filter::{AnnotationMatcher, Filter_type};
use k8s_prom::kubernetes_prometheus_config::use_k8s_https_service_auth_matcher::{
  Auth_matcher,
  KeyValue,
};
use k8s_prom::kubernetes_prometheus_config::{
  HttpServiceDiscovery,
  InclusionFilter,
  PrometheusAnnotationOverrides,
  UseK8sHttpsServiceAuthMatcher,
};
use parking_lot::Mutex;
use pretty_assertions::assert_eq;
use prometheus::labels;
use protobuf::{Message, MessageField};
use pulse_common::k8s::pods_info::container::PodsInfo;
use pulse_common::k8s::pods_info::{ContainerPort, PodsInfoSingleton};
use pulse_common::k8s::services::ServiceInfo;
use pulse_common::k8s::test::{make_node_info, make_pod_info};
use pulse_common::metadata::{Metadata, PodMetadata};
use pulse_protobuf::protos::pulse::config::common::v1::common::EnvOrInline;
use pulse_protobuf::protos::pulse::config::common::v1::common::env_or_inline::Data_type;
use pulse_protobuf::protos::pulse::config::inflow::v1::k8s_prom;
use std::collections::{HashMap, VecDeque};
use std::future::pending;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use time::ext::NumericalDuration;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use vrl::{btreemap, path};

#[tokio::test]
async fn create_endpoint() {
  let pod_info = make_pod_info(
    "some-namespace",
    "my-awesome-pod",
    &btreemap!(),
    btreemap!("prometheus.io/scrape" => "true", "prometheus.io/port" => "123"),
    HashMap::new(),
    "127.0.0.1",
    vec![],
  );
  let endpoints = create_endpoints(
    &pod_info.namespace,
    &pod_info.ip_string,
    &[],
    &[UseK8sHttpsServiceAuthMatcher {
      auth_matcher: Some(Auth_matcher::AnnotationMatcher(KeyValue {
        key: "metrics/https".into(),
        ..Default::default()
      })),
      ..Default::default()
    }],
    Some(&make_node_info()),
    Some(&pod_info),
    &pod_info.container_ports,
    None,
    None,
    &pod_info.annotations,
    PrometheusAnnotationOverrides::default_instance(),
  );
  assert_eq!(endpoints.len(), 1);
  assert!(!endpoints[0].1.use_https_k8s_service_auth);

  let pod_info = make_pod_info(
    "some-namespace",
    "my-awesome-pod",
    &btreemap!(),
    btreemap!(
      "prometheus.io/scrape" => "true",
      "prometheus.io/port" => "123",
      "metrics/https" => "true"
    ),
    HashMap::new(),
    "127.0.0.1",
    vec![],
  );
  let endpoints = create_endpoints(
    &pod_info.namespace,
    &pod_info.ip_string,
    &[],
    &[UseK8sHttpsServiceAuthMatcher {
      auth_matcher: Some(Auth_matcher::AnnotationMatcher(KeyValue {
        key: "metrics/https".into(),
        ..Default::default()
      })),
      ..Default::default()
    }],
    Some(&make_node_info()),
    Some(&pod_info),
    &pod_info.container_ports,
    None,
    None,
    &pod_info.annotations,
    PrometheusAnnotationOverrides::default_instance(),
  );
  assert_eq!(endpoints.len(), 1);
  assert!(endpoints[0].1.use_https_k8s_service_auth);

  let pod_info = make_pod_info(
    "some-namespace",
    "my-awesome-pod",
    &btreemap!(),
    btreemap!(
      "foo/scrape" => "true",
      "foo/namespace" => "other-namespace",
      "foo/path" => "/custom/path",
      "foo/scheme" => "https",
      "foo/port" => "123",
    ),
    HashMap::new(),
    "127.0.0.1",
    vec![],
  );
  let endpoints = create_endpoints(
    &pod_info.namespace,
    &pod_info.ip_string,
    &[],
    &[],
    Some(&make_node_info()),
    Some(&pod_info),
    &pod_info.container_ports,
    None,
    None,
    &pod_info.annotations,
    &PrometheusAnnotationOverrides {
      scrape: Some("foo/scrape".into()),
      namespace: Some("foo/namespace".into()),
      path: Some("foo/path".into()),
      scheme: Some("foo/scheme".into()),
      port: Some("foo/port".into()),
      ..Default::default()
    },
  );
  assert_eq!(endpoints.len(), 1);
  assert_eq!(
    endpoints[0].1,
    PromEndpoint {
      address: "127.0.0.1".into(),
      port: 123,
      path: "/custom/path".into(),
      scheme: "https".into(),
      metadata: Some(Arc::new(Metadata::new(
        Some(&make_node_info()),
        Some(PodMetadata {
          namespace: "other-namespace",
          pod_name: "my-awesome-pod",
          pod_ip: "127.0.0.1",
          pod_labels: &btreemap!(),
          pod_annotations: &btreemap!(
            "foo/scrape" => "true",
            "foo/namespace" => "other-namespace",
            "foo/path" => "/custom/path",
            "foo/scheme" => "https",
            "foo/port" => "123",
          ),
          service: None,
        }),
        Some("127.0.0.1:123".to_string()),
      ))),
      use_https_k8s_service_auth: false,
      extra_tags: vec![],
    }
  );
}

#[tokio::test]
async fn scrape_path() {
  let pod_info = make_pod_info(
    "some-namespace",
    "my-awesome-pod",
    &btreemap!(),
    btreemap!(
      "prometheus.io/scrape" => "true",
      "prometheus.io/port" => "123"
    ),
    HashMap::new(),
    "127.0.0.1",
    vec![],
  );
  let endpoints = create_endpoints(
    &pod_info.namespace,
    &pod_info.ip_string,
    &[],
    &[],
    Some(&make_node_info()),
    Some(&pod_info),
    &pod_info.container_ports,
    None,
    None,
    &pod_info.annotations,
    PrometheusAnnotationOverrides::default_instance(),
  );
  assert_eq!(endpoints[0].1.path, "/metrics");

  let pod_info = make_pod_info(
    "some-namespace",
    "my-awesome-pod",
    &btreemap!(),
    btreemap!(
      "prometheus.io/scrape" => "true",
      "prometheus.io/port" => "123",
      "prometheus.io/path" => "metrics"
    ),
    HashMap::new(),
    "127.0.0.1",
    vec![],
  );
  let endpoints = create_endpoints(
    &pod_info.namespace,
    &pod_info.ip_string,
    &[],
    &[],
    Some(&make_node_info()),
    Some(&pod_info),
    &pod_info.container_ports,
    None,
    None,
    &pod_info.annotations,
    PrometheusAnnotationOverrides::default_instance(),
  );
  assert_eq!(endpoints[0].1.path, "/metrics");

  let pod_info = make_pod_info(
    "some-namespace",
    "my-awesome-pod",
    &btreemap!(),
    btreemap!(
      "prometheus.io/scrape" => "true",
      "prometheus.io/port" => "123",
      "prometheus.io/path" => "/custom/path"
    ),
    HashMap::new(),
    "127.0.0.1",
    vec![],
  );
  let endpoints = create_endpoints(
    &pod_info.namespace,
    &pod_info.ip_string,
    &[],
    &[],
    Some(&make_node_info()),
    Some(&pod_info),
    &pod_info.container_ports,
    None,
    None,
    &pod_info.annotations,
    PrometheusAnnotationOverrides::default_instance(),
  );
  assert_eq!(endpoints[0].1.path, "/custom/path");
}

#[tokio::test]
async fn multiple_ports() {
  let mut initial_state = PodsInfo::default();
  initial_state.insert(
    make_pod_info(
      "some-namespace",
      "my-awesome-pod",
      &btreemap!(),
      btreemap!("prometheus.io/scrape" => "true", "prometheus.io/port" => "123,124"),
      HashMap::new(),
      "127.0.0.1",
      vec![],
    ),
    true,
  );
  let (_tx, rx) = tokio::sync::watch::channel(initial_state);
  let pods_info = Arc::new(PodsInfoSingleton::new(rx, make_node_info().into())).make_owned();
  let mut target = KubePodTarget {
    inclusion_filters: vec![],
    use_k8s_https_service_auth_matchers: vec![],
    pods_info,
    prom_annotation_overrides: PrometheusAnnotationOverrides::default(),
  };
  let endpoints = target.get();
  assert_eq!(
    endpoints.keys().sorted().collect_vec(),
    &[
      "some-namespace//my-awesome-pod/123/1010714942843299961",
      "some-namespace//my-awesome-pod/124/6909625251189359321"
    ]
  );
}

#[tokio::test]
async fn multiple_ports_with_space() {
  let mut initial_state = PodsInfo::default();
  initial_state.insert(
    make_pod_info(
      "some-namespace",
      "my-awesome-pod",
      &btreemap!(),
      btreemap!("prometheus.io/scrape" => "true", "prometheus.io/port" => "123, 124"),
      HashMap::new(),
      "127.0.0.1",
      vec![],
    ),
    true,
  );
  let (_tx, rx) = tokio::sync::watch::channel(initial_state);
  let pods_info = Arc::new(PodsInfoSingleton::new(rx, make_node_info().into())).make_owned();
  let mut target = KubePodTarget {
    inclusion_filters: vec![],
    use_k8s_https_service_auth_matchers: vec![],
    pods_info,
    prom_annotation_overrides: PrometheusAnnotationOverrides::default(),
  };
  let endpoints = target.get();
  assert_eq!(
    endpoints.keys().sorted().collect_vec(),
    &[
      "some-namespace//my-awesome-pod/123/12969999904310312118",
      "some-namespace//my-awesome-pod/124/5908028683721631658"
    ]
  );
}

#[tokio::test]
async fn annotation_inclusion_filter() {
  let mut initial_state = PodsInfo::default();
  initial_state.insert(
    make_pod_info(
      "some-namespace",
      "my-awesome-pod",
      &btreemap!(),
      btreemap!("match_annotation" => "true", "port_annotation" => "123"),
      HashMap::new(),
      "127.0.0.1",
      vec![],
    ),
    true,
  );
  let (_tx, rx) = tokio::sync::watch::channel(initial_state);
  let pods_info = Arc::new(PodsInfoSingleton::new(rx, make_node_info().into())).make_owned();
  let mut target = KubePodTarget {
    inclusion_filters: vec![InclusionFilter {
      filter_type: Some(Filter_type::AnnotationMatcher(AnnotationMatcher {
        inclusion_annotation_name: "match_annotation".into(),
        inclusion_annotation_regex: "true".into(),
        port_annotation_name: "port_annotation".into(),
        ..Default::default()
      })),
      ..Default::default()
    }],
    use_k8s_https_service_auth_matchers: vec![],
    pods_info,
    prom_annotation_overrides: PrometheusAnnotationOverrides::default(),
  };
  let endpoints = target.get();
  assert_eq!(
    endpoints.keys().sorted().collect_vec(),
    &["some-namespace//my-awesome-pod/123/110421865553222315"]
  );
}

#[tokio::test]
async fn inclusion_filters() {
  let mut initial_state = PodsInfo::default();
  initial_state.insert(
    make_pod_info(
      "some-namespace",
      "my-awesome-pod",
      &btreemap!(),
      btreemap!(),
      HashMap::new(),
      "127.0.0.1",
      vec![
        ContainerPort {
          name: "scrape_hello".to_string(),
          port: 123,
        },
        ContainerPort {
          name: "scrape_world".to_string(),
          port: 124,
        },
        ContainerPort {
          name: "other".to_string(),
          port: 125,
        },
      ],
    ),
    true,
  );
  let (_tx, rx) = tokio::sync::watch::channel(initial_state);
  let pods_info = Arc::new(PodsInfoSingleton::new(rx, make_node_info().into())).make_owned();
  let mut target = KubePodTarget {
    inclusion_filters: vec![InclusionFilter {
      filter_type: Some(Filter_type::ContainerPortNameRegex("scrape_.*".into())),
      ..Default::default()
    }],
    use_k8s_https_service_auth_matchers: vec![],
    pods_info,
    prom_annotation_overrides: PrometheusAnnotationOverrides::default(),
  };
  let endpoints = target.get();
  assert_eq!(
    endpoints.keys().sorted().collect_vec(),
    &[
      "some-namespace//my-awesome-pod/123/6545109170995583408",
      "some-namespace//my-awesome-pod/124/7303487244468715525"
    ]
  );
}

#[tokio::test]
async fn test_scheme_annotation() {
  // Test default scheme (http)
  let pod_info = make_pod_info(
    "some-namespace",
    "my-awesome-pod",
    &btreemap!(),
    btreemap!(
      "prometheus.io/scrape" => "true",
      "prometheus.io/port" => "123"
    ),
    HashMap::new(),
    "127.0.0.1",
    vec![],
  );
  let endpoints = create_endpoints(
    &pod_info.namespace,
    &pod_info.ip_string,
    &[],
    &[],
    Some(&make_node_info()),
    Some(&pod_info),
    &pod_info.container_ports,
    None,
    None,
    &pod_info.annotations,
    PrometheusAnnotationOverrides::default_instance(),
  );
  assert_eq!(endpoints[0].1.scheme, "http");

  // Test explicit http scheme
  let pod_info = make_pod_info(
    "some-namespace",
    "my-awesome-pod",
    &btreemap!(),
    btreemap!(
      "prometheus.io/scrape" => "true",
      "prometheus.io/port" => "123",
      "prometheus.io/scheme" => "http"
    ),
    HashMap::new(),
    "127.0.0.1",
    vec![],
  );
  let endpoints = create_endpoints(
    &pod_info.namespace,
    &pod_info.ip_string,
    &[],
    &[],
    Some(&make_node_info()),
    Some(&pod_info),
    &pod_info.container_ports,
    None,
    None,
    &pod_info.annotations,
    PrometheusAnnotationOverrides::default_instance(),
  );
  assert_eq!(endpoints[0].1.scheme, "http");

  // Test explicit https scheme
  let pod_info = make_pod_info(
    "some-namespace",
    "my-awesome-pod",
    &btreemap!(),
    btreemap!(
      "prometheus.io/scrape" => "true",
      "prometheus.io/port" => "123",
      "prometheus.io/scheme" => "https"
    ),
    HashMap::new(),
    "127.0.0.1",
    vec![],
  );
  let endpoints = create_endpoints(
    &pod_info.namespace,
    &pod_info.ip_string,
    &[],
    &[],
    Some(&make_node_info()),
    Some(&pod_info),
    &pod_info.container_ports,
    None,
    None,
    &pod_info.annotations,
    PrometheusAnnotationOverrides::default_instance(),
  );
  assert_eq!(endpoints[0].1.scheme, "https");
}

#[tokio::test]
async fn test_kube_pod_target_endpoint() {
  let mut initial_state = PodsInfo::default();
  initial_state.insert(
    make_pod_info(
      "some-namespace",
      "my-awesome-pod",
      &btreemap!(),
      btreemap!("prometheus.io/scrape" => "true"),
      HashMap::new(),
      "127.0.0.1",
      vec![],
    ),
    true,
  );

  let (_tx, rx) = tokio::sync::watch::channel(initial_state);
  let pods_info = Arc::new(PodsInfoSingleton::new(rx, make_node_info().into())).make_owned();
  let mut target = KubePodTarget {
    inclusion_filters: vec![],
    use_k8s_https_service_auth_matchers: vec![],
    pods_info,
    prom_annotation_overrides: PrometheusAnnotationOverrides::default(),
  };
  let endpoints = target.get();
  assert_eq!(endpoints.len(), 1);

  assert_eq!(
    endpoints
      .get("some-namespace//my-awesome-pod/9090/367898524252554759")
      .unwrap_or_else(|| panic!("actual endpoints: {endpoints:?}"))
      .metadata
      .as_ref()
      .unwrap()
      .value()
      .get(path!("k8s", "namespace"))
      .unwrap()
      .as_str()
      .unwrap(),
    "some-namespace"
  );
  assert_eq!(
    endpoints["some-namespace//my-awesome-pod/9090/367898524252554759"]
      .metadata
      .as_ref()
      .unwrap()
      .value()
      .get(path!("k8s", "pod", "name"))
      .unwrap()
      .as_str()
      .unwrap(),
    "my-awesome-pod"
  );
}

#[tokio::test]
async fn test_unavailable() {
  let mut setup = Setup::new(SetupBuilder::default().emit_up_metric()).await;

  make_mut(&mut setup.dispatcher)
    .expect_send()
    .times(2)
    .returning(|samples| {
      assert_eq!(samples[0].metric().get_id().name(), "up");
      assert!(float_eq!(samples[0].metric().value.to_simple(), 0.0));
    });

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
  let mut setup = Setup::new(
    SetupBuilder::default()
      .target_server()
      .invalid_status_code(),
  )
  .await;

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
  let mut setup = Setup::new(SetupBuilder::default().target_server().invalid_body()).await;

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
  let mut setup = Setup::new(SetupBuilder::default().target_server().emit_up_metric()).await;

  setup.tick_tx.send(()).await.unwrap();

  make_mut(&mut setup.dispatcher)
    .expect_send()
    .times(2)
    .returning(|samples| {
      assert_eq!(samples[0].metric().get_id().name(), "up");
      assert!(float_eq!(samples[0].metric().value.to_simple(), 1.0));
    });

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

#[tokio::test]
async fn test_http_sd() {
  let (ticker_tx, ticker_rx) = mpsc::channel(1);
  let ticker = Box::new(FakeTicker(ticker_rx));
  let mut setup = Setup::new(
    SetupBuilder::default()
      .target_server()
      .emit_up_metric()
      .endpoint_factory(Box::new(|port, shutdown| {
        async move {
          Box::new(
            HttpServiceDiscoveryEndpointTarget::new(
              HttpServiceDiscovery {
                url: MessageField::some(EnvOrInline {
                  data_type: Some(Data_type::Inline(
                    format!("http://127.0.0.1:{port}/http_sd").into(),
                  )),
                  ..Default::default()
                }),
                ..Default::default()
              },
              shutdown,
              ticker,
            )
            .await,
          ) as Box<dyn EndpointProvider>
        }
        .boxed()
      })),
  )
  .await;

  setup.tick_tx.send(()).await.unwrap();

  make_mut(&mut setup.dispatcher)
    .expect_send()
    .times(2)
    .returning(|samples| {
      assert_eq!(
        samples[0].metric().get_id().tags()[0],
        TagValue {
          tag: "hello".into(),
          value: "world".into()
        }
      );
      assert!(float_eq!(samples[0].metric().value.to_simple(), 1.0));
    });

  setup
    .stats_helper
    .wait_for_counter_eq(1, "test:scrape_complete", &labels! {})
    .await;

  assert_eq!(setup.server.1.load(Ordering::SeqCst), 1);
  ticker_tx.send(()).await.unwrap();

  setup.tick_tx.send(()).await.unwrap();
  setup
    .stats_helper
    .wait_for_counter_eq(2, "test:scrape_complete", &labels! {})
    .await;

  setup.shutdown().await;
}

#[tokio::test]
async fn test_kube_endpoints_target_auth_matchers() {
  let mut initial_state = PodsInfo::default();
  let mut services = HashMap::new();
  services.insert(
    "test-service".to_string(),
    Arc::new(ServiceInfo {
      name: "test-service".to_string(),
      maybe_service_port: Some(IntOrString::Int(9090)),
      annotations: btreemap!("prometheus.io/scrape" => "true"),
      selector: btreemap!(),
    }),
  );
  initial_state.insert(
    make_pod_info(
      "some-namespace",
      "my-awesome-pod",
      &btreemap!(),
      btreemap!("auth-annotation" => "true"),
      services,
      "127.0.0.1",
      vec![],
    ),
    true,
  );

  let (_tx, rx) = tokio::sync::watch::channel(initial_state);
  let pods_info = Arc::new(PodsInfoSingleton::new(rx, make_node_info().into())).make_owned();
  let mut target = KubeEndpointsTarget {
    pods_info,
    use_k8s_https_service_auth_matchers: vec![UseK8sHttpsServiceAuthMatcher {
      auth_matcher: Some(Auth_matcher::AnnotationMatcher(KeyValue {
        key: "auth-annotation".into(),
        value: None,
        ..Default::default()
      })),
      ..Default::default()
    }],
  };
  let endpoints = target.get();
  assert_eq!(endpoints.len(), 1);

  let endpoint = &endpoints
    .get("some-namespace/test-service/my-awesome-pod/9090/16489899550189509062")
    .unwrap_or_else(|| panic!("actual endpoints: {endpoints:?}"));
  assert!(
    endpoint.use_https_k8s_service_auth,
    "HTTPS K8s service auth should be enabled"
  );
  assert_eq!(endpoint.port, 9090);
  assert_eq!(endpoint.address, "127.0.0.1");
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
      PromEndpoint::new(
        "localhost".to_string(),
        if self.target_server {
          self.port.into()
        } else {
          1234
        },
        String::new(),
        Some(Arc::new(Metadata::new(
          Some(&make_node_info()),
          Some(PodMetadata {
            namespace: "namespace",
            pod_name: "pod",
            pod_ip: "ip",
            pod_labels: &btreemap!(),
            pod_annotations: &btreemap!(),
            service: None,
          }),
          None,
        ))),
        false,
        "http".to_string(),
        vec![],
      ),
    )]
    .into()
  }

  async fn changed(&mut self) {
    pending::<()>().await;
  }
}

//
// SetupBuilder
//

type EndpointFactoryFuture = Pin<Box<dyn Future<Output = Box<dyn EndpointProvider>>>>;
#[derive(Default)]
#[allow(clippy::struct_excessive_bools)]
struct SetupBuilder {
  target_server: bool,
  invalid_status_code: bool,
  invalid_body: bool,
  emit_up_metric: bool,
  endpoint_factory: Option<Box<dyn FnOnce(u16, ComponentShutdown) -> EndpointFactoryFuture>>,
}

impl SetupBuilder {
  const fn target_server(mut self) -> Self {
    self.target_server = true;
    self
  }

  const fn invalid_status_code(mut self) -> Self {
    self.invalid_status_code = true;
    self
  }

  const fn invalid_body(mut self) -> Self {
    self.invalid_body = true;
    self
  }

  const fn emit_up_metric(mut self) -> Self {
    self.emit_up_metric = true;
    self
  }

  fn endpoint_factory(
    mut self,
    factory: Box<dyn FnOnce(u16, ComponentShutdown) -> EndpointFactoryFuture>,
  ) -> Self {
    self.endpoint_factory = Some(factory);
    self
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
  dispatcher: Arc<MockPipelineDispatch>,
}

impl Setup {
  async fn new(builder: SetupBuilder) -> Self {
    let stats_helper = bd_server_stats::test::util::stats::Helper::new();
    let stats = Stats::new(&stats_helper.collector().scope("test"));
    let (tick_tx, tick_rx) = tokio::sync::mpsc::channel(1);
    let ticker_factory = Arc::new(FakeTickerFactory::default());
    ticker_factory.add_rx(tick_rx);

    let server = TestPromServer::start(
      if builder.invalid_status_code {
        StatusCode::BAD_GATEWAY
      } else {
        StatusCode::OK
      },
      if builder.invalid_body {
        Some("not a prom response".to_string())
      } else {
        None
      },
    )
    .await;

    let shutdown_trigger = ComponentShutdownTrigger::default();
    let dispatcher = Arc::new(MockPipelineDispatch::new());
    let tls_config = None;
    let scraper = Scraper::<TestDurationJitter>::create(
      "test".to_string(),
      stats,
      dispatcher.clone(),
      shutdown_trigger.make_handle(),
      if let Some(factory) = builder.endpoint_factory {
        factory(server.0, shutdown_trigger.make_shutdown()).await
      } else {
        Box::new(FakeEndpointProvider {
          target_server: builder.target_server,
          port: server.0,
        })
      },
      0.seconds(),
      Box::new(move || ticker_factory.make_ticker()),
      builder.emit_up_metric,
      tls_config,
      15.seconds(),
    )
    .unwrap();
    scraper.start().await;

    Self {
      stats_helper,
      tick_tx,
      server,
      shutdown_trigger: Some(shutdown_trigger),
      shutdown_called: false,
      dispatcher,
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
  port: u16,
}

impl TestPromServer {
  async fn start(
    response_code: axum::http::StatusCode,
    body: Option<String>,
  ) -> (u16, Arc<AtomicU64>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();

    let port = listener.local_addr().unwrap().port();
    log::debug!("starting test server on port {port}");

    let calls = Arc::new(AtomicU64::new(0));
    let server = Arc::new(Self {
      calls: calls.clone(),
      response_code,
      body,
      port,
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
      .route("/metrics", get(metrics))
      .route("/http_sd", get(http_sd))
      .with_state(self)
  }
}

async fn http_sd(State(server): State<Arc<TestPromServer>>) -> Response {
  log::debug!("HTTP SD called");
  serde_json::to_string(&vec![TargetBlock {
    targets: vec![format!("127.0.0.1:{}", server.port)],
    labels: btreemap!("hello" => "world"),
  }])
  .unwrap()
  .into_response()
}

async fn metrics(State(server): State<Arc<TestPromServer>>) -> Response {
  server.calls.fetch_add(1, Ordering::SeqCst);

  let body = server.body.clone();

  let body = body.map_or_else(Body::empty, std::convert::Into::into);

  let mut response = Response::new(body);
  *response.status_mut() = server.response_code;
  response
}

#[test]
fn test_create_endpoints_port_resolution() {
  // Test basic pod with container port
  let pod_info = make_pod_info(
    "default",
    "test-pod",
    &btreemap!(),
    btreemap!("prometheus.io/scrape" => "true"),
    HashMap::new(),
    "10.0.0.1",
    vec![ContainerPort {
      name: "metrics".to_string(),
      port: 9101,
    }],
  );

  let endpoints = create_endpoints(
    &pod_info.namespace,
    &pod_info.ip_string,
    &[],
    &[],
    Some(&make_node_info()),
    Some(&pod_info),
    &pod_info.container_ports,
    None,
    None,
    &pod_info.annotations,
    PrometheusAnnotationOverrides::default_instance(),
  );
  assert_eq!(endpoints.len(), 1);
  assert_eq!(endpoints[0].1.port, 9090);

  // Test annotation port override
  let pod_info = make_pod_info(
    "default",
    "test-pod",
    &btreemap!(),
    btreemap!("prometheus.io/scrape" => "true", "prometheus.io/port" => "8080"),
    HashMap::new(),
    "10.0.0.1",
    vec![ContainerPort {
      name: "metrics".to_string(),
      port: 9101,
    }],
  );
  let endpoints = create_endpoints(
    &pod_info.namespace,
    &pod_info.ip_string,
    &[],
    &[],
    Some(&make_node_info()),
    Some(&pod_info),
    &pod_info.container_ports,
    None,
    None,
    &pod_info.annotations,
    PrometheusAnnotationOverrides::default_instance(),
  );
  assert_eq!(endpoints.len(), 1);
  assert_eq!(endpoints[0].1.port, 8080);

  // Test service port resolution
  // Test integer service port
  let pod_info = make_pod_info(
    "default",
    "test-pod",
    &btreemap!(),
    btreemap!("prometheus.io/scrape" => "true"),
    HashMap::new(),
    "10.0.0.1",
    vec![ContainerPort {
      name: "metrics".to_string(),
      port: 9101,
    }],
  );
  let endpoints = create_endpoints(
    &pod_info.namespace,
    &pod_info.ip_string,
    &[],
    &[],
    Some(&make_node_info()),
    Some(&pod_info),
    &pod_info.container_ports,
    None,
    Some(&IntOrString::Int(8080)),
    &pod_info.annotations,
    PrometheusAnnotationOverrides::default_instance(),
  );
  assert_eq!(endpoints.len(), 1);
  assert_eq!(endpoints[0].1.port, 8080);

  // Test string service port matching
  let pod_info = make_pod_info(
    "default",
    "test-pod",
    &btreemap!(),
    btreemap!("prometheus.io/scrape" => "true"),
    HashMap::new(),
    "10.0.0.1",
    vec![ContainerPort {
      name: "metrics".to_string(),
      port: 9101,
    }],
  );
  let endpoints = create_endpoints(
    &pod_info.namespace,
    &pod_info.ip_string,
    &[],
    &[],
    Some(&make_node_info()),
    Some(&pod_info),
    &pod_info.container_ports,
    None,
    Some(&IntOrString::String("metrics".to_string())),
    &pod_info.annotations,
    PrometheusAnnotationOverrides::default_instance(),
  );
  assert_eq!(endpoints.len(), 1);
  assert_eq!(endpoints[0].1.port, 9101);

  // Test non-matching string service port
  let pod_info = make_pod_info(
    "default",
    "test-pod",
    &btreemap!(),
    btreemap!("prometheus.io/scrape" => "true"),
    HashMap::new(),
    "10.0.0.1",
    vec![ContainerPort {
      name: "metrics".to_string(),
      port: 9101,
    }],
  );
  let endpoints = create_endpoints(
    &pod_info.namespace,
    &pod_info.ip_string,
    &[],
    &[],
    Some(&make_node_info()),
    Some(&pod_info),
    &pod_info.container_ports,
    None,
    Some(&IntOrString::String("non-existent".to_string())),
    &pod_info.annotations,
    PrometheusAnnotationOverrides::default_instance(),
  );
  assert_eq!(endpoints.len(), 1);
  assert_eq!(endpoints[0].1.port, 9090);

  // Test inclusion filters
  let pod_info = make_pod_info(
    "default",
    "test-pod",
    &btreemap!(),
    btreemap!("prometheus.io/scrape" => "true"),
    HashMap::new(),
    "10.0.0.1",
    vec![
      ContainerPort {
        name: "scrape_hello".to_string(),
        port: 123,
      },
      ContainerPort {
        name: "scrape_world".to_string(),
        port: 124,
      },
      ContainerPort {
        name: "other".to_string(),
        port: 125,
      },
    ],
  );

  let inclusion_filters = vec![InclusionFilter {
    filter_type: Some(Filter_type::ContainerPortNameRegex("scrape_.*".into())),
    ..Default::default()
  }];

  let endpoints = create_endpoints(
    &pod_info.namespace,
    &pod_info.ip_string,
    &inclusion_filters,
    &[],
    Some(&make_node_info()),
    Some(&pod_info),
    &pod_info.container_ports,
    None,
    None,
    &pod_info.annotations,
    PrometheusAnnotationOverrides::default_instance(),
  );
  assert_eq!(endpoints.len(), 2);
  let ports: Vec<i32> = endpoints.iter().map(|e| e.1.port).collect();
  assert!(ports.contains(&123));
  assert!(ports.contains(&124));

  // Test multiple ports from annotations
  let pod_info = make_pod_info(
    "default",
    "test-pod",
    &btreemap!(),
    btreemap!("prometheus.io/scrape" => "true", "prometheus.io/port" => "8080,9090"),
    HashMap::new(),
    "10.0.0.1",
    vec![ContainerPort {
      name: "metrics".to_string(),
      port: 9101,
    }],
  );
  let endpoints = create_endpoints(
    &pod_info.namespace,
    &pod_info.ip_string,
    &[],
    &[],
    Some(&make_node_info()),
    Some(&pod_info),
    &pod_info.container_ports,
    None,
    None,
    &pod_info.annotations,
    PrometheusAnnotationOverrides::default_instance(),
  );
  assert_eq!(endpoints.len(), 2);
  let ports: Vec<i32> = endpoints.iter().map(|e| e.1.port).collect();
  assert!(ports.contains(&8080));
  assert!(ports.contains(&9090));
}
