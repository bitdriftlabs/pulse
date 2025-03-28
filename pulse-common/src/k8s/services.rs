// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./services_test.rs"]
mod services_test;

use super::object_namespace;
use async_trait::async_trait;
use k8s_openapi::api::core::v1::{Pod, Service};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::ResourceExt;
use kube::api::ListParams;
use mockall::automock;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use tokio::time::Instant;

//
// ServiceInfo
//

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct ServiceInfo {
  pub name: String,
  pub annotations: BTreeMap<String, String>,
  pub selector: BTreeMap<String, String>,
  pub maybe_service_port: Option<IntOrString>,
}

//
// ServiceFetcher
//

#[automock]
#[async_trait]
pub trait ServiceFetcher: Send + Sync {
  async fn services_for_namespace(&self, namespace: &str) -> anyhow::Result<Vec<Service>>;
}

pub struct RealServiceFetcher;

#[async_trait]
impl ServiceFetcher for RealServiceFetcher {
  async fn services_for_namespace(&self, namespace: &str) -> anyhow::Result<Vec<Service>> {
    let client = kube::Client::try_default().await?;
    let service_api: kube::Api<Service> = kube::Api::namespaced(client, namespace);
    let services = service_api.list(&ListParams::default().match_any()).await?;
    Ok(services.items)
  }
}

//
// ServiceCache
//

pub struct ServiceCache {
  services_by_namespace: HashMap<String, PerNamespaceServices>,
  ttl_duration: time::Duration,
  service_fetcher: Box<dyn ServiceFetcher>,
}
struct PerNamespaceServices {
  services: HashMap<String, Arc<ServiceInfo>>,
  fetch_time: Instant,
}

impl ServiceCache {
  #[must_use]
  pub fn new(ttl_duration: time::Duration, service_fetcher: Box<dyn ServiceFetcher>) -> Self {
    Self {
      services_by_namespace: HashMap::new(),
      ttl_duration,
      service_fetcher,
    }
  }

  fn filter_services(services: &PerNamespaceServices, pod: &Pod) -> Vec<Arc<ServiceInfo>> {
    // TODO(snowp): There is a use case for collecting metrics for each active service, so this
    // should return all matching services instead.
    services
      .services
      .iter()
      .filter_map(|(_, service)| {
        if matching_label_selector(&service.selector, pod.labels()) {
          Some(service)
        } else {
          None
        }
      })
      .cloned()
      .collect()
  }

  /// Attempts to resolve an active service for the provided pod. This is done by attempting to
  /// match the pod against all active services in the namespace of the pod.
  pub async fn find_services(&mut self, pod: &Pod) -> Vec<Arc<ServiceInfo>> {
    if let Some(services) = self
      .services_by_namespace
      .get(object_namespace(&pod.metadata))
    {
      // Only consider services that have been fetched within the TTL duration.
      if Instant::now() - services.fetch_time < self.ttl_duration {
        return Self::filter_services(services, pod);
      }
      log::debug!(
        "services for namespace {} have expired, refetching",
        object_namespace(&pod.metadata)
      );
    }

    log::debug!(
      "fetching services for namespace {}",
      object_namespace(&pod.metadata)
    );
    let services = self
      .service_fetcher
      .services_for_namespace(object_namespace(&pod.metadata))
      .await
      .inspect_err(|e| {
        log::warn!(
          "failed to fetch services for namespace {}: {}",
          object_namespace(&pod.metadata),
          e
        );
      })
      .unwrap_or_default()
      .into_iter()
      .map(|service| {
        let maybe_service_port = service
          .spec
          .as_ref()
          .and_then(|spec| spec.ports.as_ref())
          .and_then(|ports| ports.first())
          .and_then(|port| port.target_port.clone());

        (
          service.metadata.name.clone().unwrap_or_default(),
          Arc::new(ServiceInfo {
            name: service.metadata.name.clone().unwrap_or_default(),
            annotations: service.annotations().clone(),
            selector: service
              .spec
              .and_then(|spec| spec.selector)
              .unwrap_or_default(),
            maybe_service_port,
          }),
        )
      })
      .collect();
    let services = PerNamespaceServices {
      services,
      fetch_time: Instant::now(),
    };
    let result = Self::filter_services(&services, pod);
    self
      .services_by_namespace
      .insert(object_namespace(&pod.metadata).to_string(), services);

    result
  }
}

/// Matches the provided label selector against a set of labels, returning true if the selector
/// matches the labels.
fn matching_label_selector(
  label_selector: &BTreeMap<String, String>,
  labels: &BTreeMap<String, String>,
) -> bool {
  for (k, v) in label_selector {
    let Some(value) = labels.get(k) else {
      return false;
    };

    if value != v {
      return false;
    }
  }

  true
}
