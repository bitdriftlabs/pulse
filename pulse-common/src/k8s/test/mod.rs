// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::pods_info::{PodInfo, PromEndpoint, ServiceInfo};
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
  prom_endpoint: Option<PromEndpoint>,
  services: HashMap<String, Arc<ServiceInfo>>,
  ip: &str,
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
    name: name.to_string().into(),
    namespace: namespace.to_string().into(),
    prom_endpoint,
    services,
    ip: ip.parse().unwrap(),
    annotations: annotations
      .into_iter()
      .map(|(k, v)| (k.into(), v.into()))
      .collect(),
    metadata,
  }
}
