// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

pub mod endpoints;
pub mod pods_info;
pub mod services;
pub mod test;
mod watcher_base;

use anyhow::anyhow;
use k8s_openapi::api::core::v1::Node;
use kube::api::{GetParams, ObjectMeta};
use kube::{Api, ResourceExt};
use std::collections::BTreeMap;
use std::sync::Arc;

#[must_use]
pub fn missing_node_name_error() -> anyhow::Error {
  anyhow!("Kubernetes node name not specified in bootstrap")
}

/// Returns the namespace for the provided object.
fn object_namespace(meta: &ObjectMeta) -> &str {
  meta.namespace.as_deref().unwrap_or("default")
}

//
// NodeInfo
//

/// Information about a Kubernetes node fetched via the kube API.
pub struct NodeInfo {
  pub name: String,
  pub ip: String,
  /// The port that can be used to communicate with the kubelet.
  pub kubelet_port: i32,
  pub labels: BTreeMap<String, String>,
  pub annotations: BTreeMap<String, String>,
}

impl NodeInfo {
  pub async fn new(node_name: &str) -> anyhow::Result<Arc<Self>> {
    let client = kube::Client::try_default().await?;
    let node_api: Api<Node> = kube::Api::all(client);

    let node = node_api.get_with(node_name, &GetParams::any()).await?;
    let labels = node.labels().clone();
    let annotations = node.annotations().clone();

    let kubelet_port = node
      .status
      .as_ref()
      .and_then(|s| s.daemon_endpoints.as_ref())
      .and_then(|d| d.kubelet_endpoint.as_ref())
      .ok_or_else(|| anyhow!("Kubelet endpoint not found in node status"))?
      .port;

    let ip = node
      .status
      .and_then(|s| s.addresses)
      .and_then(|addresses| {
        addresses
          .into_iter()
          .find(|address| address.type_ == "InternalIP")
          .map(|address| address.address)
      })
      .ok_or_else(|| anyhow!("Node IP not found in node status"))?;

    Ok(Arc::new(Self {
      name: node_name.to_string(),
      ip,
      kubelet_port,
      labels,
      annotations,
    }))
  }
}
