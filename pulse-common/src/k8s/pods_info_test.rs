// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::PodsInfoCache;
use crate::k8s::pods_info::{ContainerPort, PodsInfo, ServiceInfo};
use crate::k8s::services::{MockServiceFetcher, ServiceCache};
use crate::k8s::test::{make_node_info, make_object_meta, make_pod_info};
use bd_server_stats::stats::Collector;
use bd_server_stats::test::util::stats::Helper;
use bootstrap::v1::bootstrap::{KubernetesBootstrapConfig, kubernetes_bootstrap_config};
use k8s_openapi::api::core::v1::{self, Container, Pod, PodSpec, PodStatus, Service, ServiceSpec};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kubernetes_bootstrap_config::host_network_pod_by_ip_filter::Filter_type;
use kubernetes_bootstrap_config::{HostNetworkPodByIpFilter, MetadataMatcher};
use pretty_assertions::assert_eq;
use prometheus::labels;
use pulse_protobuf::protos::pulse::config::bootstrap;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use time::ext::NumericalDuration;
use vrl::btreemap;

#[allow(clippy::unnecessary_wraps)]
fn make_pod_status(ip: &str, phase: &str) -> Option<PodStatus> {
  Some(PodStatus {
    pod_ip: Some(ip.to_string()),
    host_ip: Some("node_ip".to_string()),
    phase: Some(phase.to_string()),
    ..Default::default()
  })
}

#[tokio::test]
async fn host_network_filter() {
  let (tx, rx) = tokio::sync::watch::channel(PodsInfo::default());
  let mut cache = PodsInfoCache::new(
    make_node_info().into(),
    tx,
    KubernetesBootstrapConfig {
      host_network_pod_by_ip_filter: Some(HostNetworkPodByIpFilter {
        filter_type: Some(Filter_type::LabelMatcher(MetadataMatcher {
          name: "hello".into(),
          value_regex: "world".into(),
          ..Default::default()
        })),
        ..Default::default()
      })
      .into(),
      ..Default::default()
    },
    None,
    &Collector::default(),
  )
  .unwrap();

  // The first pod will be excluded based on the filter.
  let pod1 = Pod {
    metadata: make_object_meta("my-awesome-pod", btreemap! {}, btreemap! {}),
    status: make_pod_status("127.0.0.1", "Running"),
    spec: Some(PodSpec {
      host_network: Some(true),
      ..Default::default()
    }),
  };
  cache.apply_pod(&pod1).await;
  assert!(rx.borrow().by_ip(&"127.0.0.1".parse().unwrap()).is_none());

  // The second pod matches and will be included.
  let pod2 = Pod {
    metadata: make_object_meta(
      "my-awesome-pod2",
      btreemap! {
        "hello" => "world".to_string()
      },
      btreemap! {},
    ),
    status: make_pod_status("127.0.0.1", "Running"),
    spec: Some(PodSpec {
      host_network: Some(true),
      ..Default::default()
    }),
  };
  cache.apply_pod(&pod2).await;
  assert_eq!(
    "my-awesome-pod2",
    rx.borrow()
      .by_ip(&"127.0.0.1".parse().unwrap())
      .unwrap()
      .name
  );

  // Remove pod1 and make sure we still have pod2 in the map.
  cache.remove_pod(&pod1);
  assert_eq!(
    "my-awesome-pod2",
    rx.borrow()
      .by_ip(&"127.0.0.1".parse().unwrap())
      .unwrap()
      .name
  );

  // Remove pod2 and make sure we don't have anything in the map.
  cache.remove_pod(&pod2);
  assert!(rx.borrow().by_ip(&"127.0.0.1".parse().unwrap()).is_none());
}

#[tokio::test]
async fn host_network() {
  let (tx, rx) = tokio::sync::watch::channel(PodsInfo::default());
  let helper = Helper::default();
  let mut cache = PodsInfoCache::new(
    make_node_info().into(),
    tx,
    KubernetesBootstrapConfig::default(),
    None,
    helper.collector(),
  )
  .unwrap();

  let pod1 = Pod {
    metadata: make_object_meta("my-awesome-pod", btreemap! {}, btreemap! {}),
    status: make_pod_status("127.0.0.1", "Running"),
    spec: Some(PodSpec {
      host_network: Some(true),
      ..Default::default()
    }),
  };
  cache.apply_pod(&pod1).await;

  let pod2 = Pod {
    metadata: make_object_meta("my-awesome-pod2", btreemap! {}, btreemap! {}),
    status: make_pod_status("127.0.0.1", "Running"),
    spec: Some(PodSpec {
      host_network: Some(true),
      ..Default::default()
    }),
  };
  cache.apply_pod(&pod2).await;

  // We should still have the first by IP.
  helper.assert_counter_eq(
    1,
    "k8s_pods_info:host_network_pod_by_ip_conflict",
    &labels! {},
  );
  assert_eq!(
    "my-awesome-pod",
    rx.borrow()
      .by_ip(&"127.0.0.1".parse().unwrap())
      .unwrap()
      .name
  );

  // Remove the second pod, which should not remove the first from the map.
  cache.remove_pod(&pod2);
  assert_eq!(
    "my-awesome-pod",
    rx.borrow()
      .by_ip(&"127.0.0.1".parse().unwrap())
      .unwrap()
      .name
  );

  // Now removing pod1 should remove it from the map.
  cache.remove_pod(&pod1);
  assert!(rx.borrow().by_ip(&"127.0.0.1".parse().unwrap()).is_none());
}

#[tokio::test]
async fn pod_cache() {
  let (tx, mut rx) = tokio::sync::watch::channel(PodsInfo::default());
  let mut fetcher = MockServiceFetcher::new();

  fetcher
    .expect_services_for_namespace()
    .withf(|namespace| namespace == "default")
    .returning(|_| {
      Ok(vec![
        Service {
          metadata: make_object_meta(
            "svc1",
            btreemap!(),
            btreemap! {
              "prometheus.io/port" => "1234",
              "prometheus.io/scrape" => "true",
              "bitdrift.io/team_name" => "team",
              "not_supported.io/team_name" => "team",
            },
          ),
          spec: Some(ServiceSpec {
            selector: Some(btreemap! {
              "service" => "svc1"
            }),
            ..Default::default()
          }),
          ..Default::default()
        },
        Service {
          metadata: make_object_meta(
            "svc2",
            btreemap!(),
            btreemap! {
              "prometheus.io/scrape" => "true",
              "prometheus.io/namespace" => "another_namespace"
            },
          ),
          spec: Some(ServiceSpec {
            selector: Some(btreemap! {
              "service" => "svc2"
            }),
            ports: Some(vec![v1::ServicePort {
              target_port: Some(IntOrString::Int(4321)),
              ..Default::default()
            }]),
            ..Default::default()
          }),
          ..Default::default()
        },
      ])
    });

  let services = ServiceCache::new(15.minutes(), Box::new(fetcher));
  let mut cache = PodsInfoCache::new(
    make_node_info().into(),
    tx,
    KubernetesBootstrapConfig::default(),
    Some(services),
    &Collector::default(),
  )
  .unwrap();

  cache
    .apply_pod(&Pod {
      metadata: make_object_meta(
        "my-awesome-pod",
        btreemap! {
          "service" => "svc1"
        },
        btreemap! {
          "prometheus.io/scrape" => "true",
        },
      ),
      status: make_pod_status("127.0.0.1", "Running"),
      spec: Some(PodSpec {
        containers: vec![Container {
          ports: Some(vec![v1::ContainerPort {
            container_port: 1234,
            name: Some("http".to_string()),
            ..Default::default()
          }]),
          ..Default::default()
        }],
        ..Default::default()
      }),
    })
    .await;
  cache
    .apply_pod(&Pod {
      metadata: make_object_meta(
        "my-second-awesome-pod",
        btreemap! {
          "service" => "svc2"
        },
        btreemap! {},
      ),
      status: make_pod_status("127.0.0.2", "Running"),
      ..Default::default()
    })
    .await;
  cache
    .apply_pod(&Pod {
      metadata: make_object_meta("my-serviceless-pod", btreemap! {}, btreemap! {}),
      status: make_pod_status("127.0.0.3", "Running"),
      ..Default::default()
    })
    .await;

  let current = rx.borrow_and_update();

  assert_eq!(
    &make_pod_info(
      "default",
      "my-awesome-pod",
      &btreemap!("service" => "svc1"),
      btreemap!("prometheus.io/scrape" => "true"),
      HashMap::from([(
        "svc1".to_string(),
        Arc::new(ServiceInfo {
          name: "svc1".to_string(),
          annotations: btreemap! {
            "prometheus.io/port" => "1234",
            "prometheus.io/scrape" => "true",
            "bitdrift.io/team_name" => "team",
            "not_supported.io/team_name" => "team",
          },
          selector: btreemap! {
            "service" => "svc1"
          },
          maybe_service_port: None,
        }),
      )]),
      "127.0.0.1",
      vec![ContainerPort {
        name: "http".to_string(),
        port: 1234,
      }]
    ),
    current
      .by_name("default", "my-awesome-pod")
      .unwrap()
      .as_ref()
  );
  assert_eq!(
    "my-awesome-pod",
    current
      .by_ip(&"127.0.0.1".parse().unwrap())
      .unwrap()
      .name
      .as_str()
  );
  assert_eq!(
    "my-awesome-pod",
    current
      .by_ip(&"::ffff:127.0.0.1".parse().unwrap())
      .unwrap()
      .name
      .as_str()
  );

  assert_eq!(
    &make_pod_info(
      "default",
      "my-second-awesome-pod",
      &btreemap!("service" => "svc2"),
      BTreeMap::default(),
      HashMap::from([(
        "svc2".to_string(),
        Arc::new(ServiceInfo {
          name: "svc2".to_string(),
          annotations: btreemap! {
            "prometheus.io/scrape" => "true",
            "prometheus.io/namespace" => "another_namespace"
          },
          selector: btreemap! {
            "service" => "svc2"
          },
          maybe_service_port: Some(IntOrString::Int(4321)),
        }),
      )]),
      "127.0.0.2",
      Vec::new()
    ),
    current
      .by_name("default", "my-second-awesome-pod")
      .unwrap()
      .as_ref()
  );
  assert_eq!(
    "my-second-awesome-pod",
    current
      .by_ip(&"127.0.0.2".parse().unwrap())
      .unwrap()
      .name
      .as_str()
  );

  assert_eq!(
    &make_pod_info(
      "default",
      "my-serviceless-pod",
      &BTreeMap::default(),
      BTreeMap::default(),
      HashMap::new(),
      "127.0.0.3",
      Vec::new()
    ),
    current
      .by_name("default", "my-serviceless-pod")
      .unwrap()
      .as_ref()
  );
  assert_eq!(
    "my-serviceless-pod",
    current
      .by_ip(&"127.0.0.3".parse().unwrap())
      .unwrap()
      .name
      .as_str()
  );

  drop(current);
  cache
    .apply_pod(&Pod {
      metadata: make_object_meta(
        "my-awesome-pod",
        btreemap! {
          "service" => "svc1"
        },
        btreemap! {
          "prometheus.io/scrape" => "true",
        },
      ),
      status: make_pod_status("127.0.0.1", "Running"),
      ..Default::default()
    })
    .await;
  cache
    .apply_pod(&Pod {
      metadata: make_object_meta(
        "my-second-awesome-pod",
        btreemap! {
          "service" => "svc2"
        },
        btreemap! {},
      ),
      status: make_pod_status("127.0.0.2", "Running"),
      ..Default::default()
    })
    .await;
  cache
    .apply_pod(&Pod {
      metadata: make_object_meta("my-serviceless-pod", btreemap! {}, btreemap! {}),
      status: make_pod_status("127.0.0.3", "Succeeded"),
      ..Default::default()
    })
    .await;

  let current = rx.borrow_and_update();
  assert!(current.by_name("default", "my-serviceless-pod").is_none());
  assert!(current.by_ip(&"127.0.0.3".parse().unwrap()).is_none());
}
