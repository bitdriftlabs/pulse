// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

pub mod pods_info;
pub mod test;

use anyhow::anyhow;
use k8s_openapi::api::core::v1::Node;
use kube::Api;
use kube::api::GetParams;

#[must_use]
pub fn missing_node_name_error() -> anyhow::Error {
  anyhow!("Kubernetes node name not specified in bootstrap")
}

/// Information about a Kubernetes node fetched via the kube API.
pub struct NodeInfo {
  /// The port that can be used to communicate with the kubelet.
  pub kubelet_port: i32,
}

impl NodeInfo {
  pub async fn new(node: &str) -> Self {
    let client = kube::Client::try_default().await.unwrap();
    let node_api: Api<Node> = kube::Api::all(client);

    let node = node_api.get_with(node, &GetParams::any()).await.unwrap();

    let kubelet_port = node
      .status
      .unwrap()
      .daemon_endpoints
      .unwrap()
      .kubelet_endpoint
      .unwrap()
      .port;

    Self { kubelet_port }
  }
}
