// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::NodeInfo;
use super::pods_info::{ContainerPort, PodInfo};
use super::services::ServiceInfo;
use crate::metadata::{Metadata, PodMetadata};
use kube::api::ObjectMeta;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

#[must_use]
pub fn make_object_meta(
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

#[must_use]
pub fn make_node_info() -> NodeInfo {
  NodeInfo {
    name: "node_name".to_string(),
    ip: "node_ip".to_string(),
    kubelet_port: 0,
    labels: BTreeMap::default(),
    annotations: BTreeMap::default(),
  }
}

#[must_use]
#[allow(clippy::implicit_hasher)]
pub fn make_pod_info(
  namespace: &str,
  name: &str,
  labels: &BTreeMap<String, String>,
  annotations: BTreeMap<String, String>,
  services: HashMap<String, Arc<ServiceInfo>>,
  ip: &str,
  container_ports: Vec<ContainerPort>,
) -> PodInfo {
  let metadata = Arc::new(Metadata::new(
    Some(&make_node_info()),
    Some(PodMetadata {
      namespace,
      pod_name: name,
      pod_ip: ip,
      pod_labels: labels,
      pod_annotations: &annotations,
      service: None,
    }),
    None,
  ));
  PodInfo {
    name: name.to_string(),
    namespace: namespace.to_string(),
    services,
    ip: ip.parse().unwrap(),
    ip_string: ip.to_string(),
    labels: labels.clone(),
    annotations,
    metadata,
    container_ports,
  }
}
