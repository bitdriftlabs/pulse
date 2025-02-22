// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::pods_info::{ContainerPort, PodInfo, ServiceInfo};
use crate::metadata::Metadata;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

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
    namespace,
    name,
    ip,
    labels,
    &annotations,
    None,
    "node",
    "node_ip",
    None,
  ));
  PodInfo {
    name: name.to_string(),
    namespace: namespace.to_string(),
    services,
    ip: ip.parse().unwrap(),
    labels: labels.clone(),
    annotations,
    metadata,
    container_ports,
    node_name: "node".to_string(),
    node_ip: "node_ip".to_string(),
  }
}
