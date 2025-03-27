// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::k8s::NodeInfo;
use crate::k8s::pods_info::make_namespace_and_name;
use std::collections::BTreeMap;
use vrl::value;
use vrl::value::kind::Collection;
use vrl::value::{Kind, Value};

//
// Metadata
//

/// Holds additional metadata about the origin of the event data.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Metadata {
  // TODO(snowp): For now we just hard code k8s information here. Down the line we'd want this to
  // be a bit more generic, e.g. a provider / consumer contract where sources emit providers and
  // filters are consumers - this would allow us to type check the pipeline to make sure that
  // required data is made available to consumers.
  k8s_namespace_and_pod_name: Option<String>,
  value: Value,
}

pub struct PodMetadata<'a> {
  pub namespace: &'a str,
  pub pod_name: &'a str,
  pub pod_ip: &'a str,
  pub pod_labels: &'a BTreeMap<String, String>,
  pub pod_annotations: &'a BTreeMap<String, String>,
  pub service: Option<&'a str>,
}

impl Metadata {
  #[must_use]
  pub fn new(
    node_info: &NodeInfo,
    pod_metadata: Option<PodMetadata<'_>>,
    prom_scrape_address: Option<String>,
  ) -> Self {
    fn btree_to_value(values: &BTreeMap<String, String>) -> Value {
      values
        .iter()
        .map(|(key, value)| (key.clone(), value.clone().into()))
        .collect::<Value>()
    }

    let k8s_namespace_and_pod_name = pod_metadata
      .as_ref()
      .map(|p| make_namespace_and_name(p.namespace, p.pod_name));
    let namespace = pod_metadata.as_ref().map(|p| p.namespace.to_string());
    let pod_name = pod_metadata.as_ref().map(|p| p.pod_name.to_string());
    let pod_ip = pod_metadata.as_ref().map(|p| p.pod_ip.to_string());
    let pod_labels = pod_metadata.as_ref().map(|p| btree_to_value(p.pod_labels));
    let pod_annotations = pod_metadata
      .as_ref()
      .map(|p| btree_to_value(p.pod_annotations));
    let node_name = node_info.name.to_string();
    let node_ip = node_info.ip.to_string();
    let node_labels = btree_to_value(&node_info.labels);
    let node_annotations = btree_to_value(&node_info.annotations);
    let service = pod_metadata.and_then(|p| p.service.map(|s| value!({"name": s})));
    let value = value!({
      "prom": {
        "scrape": {
          "address": prom_scrape_address,
        },
      },
      "k8s": {
        "namespace": namespace,
        "service": service,
        "pod": {
          "name": pod_name,
          "ip": pod_ip,
          "labels": pod_labels,
          "annotations": pod_annotations,
        },
        "node": {
          "name": node_name,
          "ip": node_ip,
          "labels": node_labels,
          "annotations": node_annotations,
        }
      }
    });

    Self {
      k8s_namespace_and_pod_name,
      value,
    }
  }

  pub fn k8s_namespace_and_pod_name(&self) -> Option<&str> {
    self.k8s_namespace_and_pod_name.as_deref()
  }

  pub const fn value(&self) -> &Value {
    &self.value
  }

  #[must_use]
  pub fn schema() -> Kind {
    let pod_collection = Collection::empty()
      .with_known("name", Kind::bytes().or_undefined())
      .with_known(
        "labels",
        Kind::object(Collection::from_unknown(Kind::bytes())).or_undefined(),
      )
      .with_known(
        "annotations",
        Kind::object(Collection::from_unknown(Kind::bytes())).or_undefined(),
      );

    let node_collection = Collection::empty()
      .with_known("name", Kind::bytes().or_undefined())
      .with_known("ip", Kind::bytes().or_undefined())
      .with_known(
        "labels",
        Kind::object(Collection::from_unknown(Kind::bytes())).or_undefined(),
      )
      .with_known(
        "annotations",
        Kind::object(Collection::from_unknown(Kind::bytes())).or_undefined(),
      );

    let service_collection = Collection::empty().with_known("name", Kind::bytes().or_undefined());

    let prom_collection = Collection::empty().with_known(
      "scrape",
      Kind::object(Collection::empty().with_known("address", Kind::bytes().or_undefined())),
    );

    Kind::object(
      Collection::empty()
        .with_known(
          "k8s",
          Kind::object(
            Collection::empty()
              .with_known("service", Kind::object(service_collection).or_undefined())
              .with_known("namespace", Kind::bytes().or_undefined())
              .with_known("pod", Kind::object(pod_collection).or_undefined())
              .with_known("node", Kind::object(node_collection).or_undefined()),
          ),
        )
        .with_known("prom", Kind::object(prom_collection)),
    )
  }
}
