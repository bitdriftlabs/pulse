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
use k8s_openapi::api::core::v1::{self, Container, Pod, PodSpec, PodStatus, Service, ServiceSpec};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use pretty_assertions::assert_eq;
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
  let mut cache = PodsInfoCache::new(make_node_info().into(), tx, vec![], Some(services));

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
