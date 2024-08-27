// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::PodsInfoCache;
use crate::k8s::pods_info::{PodsInfo, PromEndpoint, ServiceInfo, ServiceMonitor};
use crate::k8s::test::make_pod_info;
use crate::metadata::Metadata;
use k8s_openapi::api::core::v1::{Pod, PodStatus, Service, ServicePort, ServiceSpec};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::core::ObjectMeta;
use pretty_assertions::assert_eq;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use vrl::btreemap;

fn make_pod_status(ip: &str, phase: &str) -> Option<PodStatus> {
  Some(PodStatus {
    pod_ip: Some(ip.to_string()),
    phase: Some(phase.to_string()),
    ..Default::default()
  })
}

fn make_object_meta(
  name: &str,
  labels: BTreeMap<String, String>,
  annotations: BTreeMap<String, String>,
) -> ObjectMeta {
  ObjectMeta {
    name: Some(name.to_string()),
    labels: Some(labels),
    annotations: Some(annotations),
    ..Default::default()
  }
}

#[test]
fn pod_cache() {
  let (tx, mut rx) = tokio::sync::watch::channel(PodsInfo::default());

  let mut cache = PodsInfoCache::new(tx);

  let services = ServiceMonitor::default();

  services.cache.write().services_by_namespace.insert(
    "default".to_string(),
    HashMap::from([
      (
        "svc1".to_string(),
        Service {
          metadata: make_object_meta(
            "svc1",
            btreemap! {},
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
      ),
      (
        "svc2".to_string(),
        Service {
          metadata: make_object_meta(
            "svc2",
            btreemap! {},
            btreemap! {
              "prometheus.io/scrape" => "true",
              "prometheus.io/namespace" => "another_namespace"
            },
          ),
          spec: Some(ServiceSpec {
            selector: Some(btreemap! {
              "service" => "svc2"
            }),
            ports: Some(vec![ServicePort {
              target_port: Some(IntOrString::Int(4321)),
              ..Default::default()
            }]),
            ..Default::default()
          }),
          ..Default::default()
        },
      ),
    ]),
  );

  cache.apply_pod(
    &Pod {
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
    },
    Some(&services),
  );
  cache.apply_pod(
    &Pod {
      metadata: make_object_meta(
        "my-second-awesome-pod",
        btreemap! {
          "service" => "svc2"
        },
        btreemap! {},
      ),
      status: make_pod_status("127.0.0.2", "Running"),
      ..Default::default()
    },
    Some(&services),
  );
  cache.apply_pod(
    &Pod {
      metadata: make_object_meta("my-serviceless-pod", btreemap! {}, btreemap! {}),
      status: make_pod_status("127.0.0.3", "Running"),
      ..Default::default()
    },
    Some(&services),
  );

  let current = rx.borrow_and_update();

  assert_eq!(
    &make_pod_info(
      "default",
      "my-awesome-pod",
      &btreemap!("service" => "svc1"),
      btreemap!("prometheus.io/scrape" => "true"),
      Some(PromEndpoint::new(
        "127.0.0.1".parse().unwrap(),
        9090,
        "/metrics",
        Some(Arc::new(Metadata::new(
          "default",
          "my-awesome-pod",
          &btreemap!("service" => "svc1"),
          &btreemap!("prometheus.io/scrape" => "true"),
          None
        ))),
      )),
      HashMap::from([(
        "svc1".to_string(),
        Arc::new(ServiceInfo {
          name: "svc1".to_string().into(),
          annotations: vec![(
            "bitdrift.io/team_name".to_string().into(),
            "team".to_string().into()
          )],
          prom_endpoint: Some(PromEndpoint::new(
            "127.0.0.1".parse().unwrap(),
            1234,
            "/metrics",
            Some(Arc::new(Metadata::new(
              "default",
              "my-awesome-pod",
              &btreemap!("service" => "svc1"),
              &btreemap!("prometheus.io/scrape" => "true"),
              Some("svc1")
            ))),
          )),
        }),
      )]),
      "127.0.0.1"
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
      None,
      HashMap::from([(
        "svc2".to_string(),
        Arc::new(ServiceInfo {
          name: "svc2".to_string().into(),
          annotations: Vec::new(),
          prom_endpoint: Some(PromEndpoint {
            url: "http://127.0.0.2:4321/metrics".to_string(),
            metadata: Some(Arc::new(Metadata::new(
              "another_namespace",
              "my-second-awesome-pod",
              &btreemap!("service" => "svc2"),
              &BTreeMap::default(),
              Some("svc2")
            ))),
          }),
        }),
      )]),
      "127.0.0.2"
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
      None,
      HashMap::new(),
      "127.0.0.3"
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
  cache.apply_pod(
    &Pod {
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
    },
    Some(&services),
  );
  cache.apply_pod(
    &Pod {
      metadata: make_object_meta(
        "my-second-awesome-pod",
        btreemap! {
          "service" => "svc2"
        },
        btreemap! {},
      ),
      status: make_pod_status("127.0.0.2", "Running"),
      ..Default::default()
    },
    Some(&services),
  );
  cache.apply_pod(
    &Pod {
      metadata: make_object_meta("my-serviceless-pod", btreemap! {}, btreemap! {}),
      status: make_pod_status("127.0.0.3", "Succeeded"),
      ..Default::default()
    },
    Some(&services),
  );

  let current = rx.borrow_and_update();
  assert!(current.by_name("default", "my-serviceless-pod").is_none());
  assert!(current.by_ip(&"127.0.0.3".parse().unwrap()).is_none());
}
