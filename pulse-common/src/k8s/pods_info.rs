// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./pods_info_test.rs"]
mod pods_info_test;

use self::container::PodsInfo;
use super::services::{ServiceCache, ServiceInfo};
use super::watcher_base::ResourceWatchCallbacks;
use super::{NodeInfo, missing_node_name_error, object_namespace};
use crate::k8s::services::RealServiceFetcher;
use crate::k8s::watcher_base::WatcherBase;
use crate::proto::env_or_inline_to_string;
use crate::singleton::{SingletonHandle, SingletonManager};
use async_trait::async_trait;
use bd_server_stats::stats::Collector;
use bd_shutdown::{ComponentShutdown, ComponentShutdownTriggerHandle};
use bootstrap::KubernetesBootstrapConfig;
use bootstrap::kubernetes_bootstrap_config::MetadataMatcher;
use bootstrap::kubernetes_bootstrap_config::host_network_pod_by_ip_filter::Filter_type;
use futures_util::future::BoxFuture;
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use prometheus::IntCounter;
use protobuf::Chars;
use pulse_protobuf::protos::pulse::config::bootstrap::v1::bootstrap;
use regex::Regex;
use std::collections::{BTreeMap, HashMap};
use std::net::IpAddr;
use std::sync::{Arc, OnceLock};
use time::ext::NumericalDuration;
use tokio::sync::watch::{self, Ref};

pub type K8sWatchFactory =
  Arc<dyn Fn() -> BoxFuture<'static, anyhow::Result<Arc<PodsInfoSingleton>>> + Send + Sync>;

#[must_use]
pub fn make_namespace_and_name(namespace: &str, name: &str) -> String {
  format!("{namespace}/{name}")
}

//
// OwnedPodsInfoSingleton
//

// This is an owned handle that makes sure the backing singleton is not dropped and actually
// shared.
#[derive(Clone)]
pub struct OwnedPodsInfoSingleton {
  parent: Arc<PodsInfoSingleton>,
  rx: watch::Receiver<PodsInfo>,
}

impl OwnedPodsInfoSingleton {
  #[must_use]
  pub fn borrow(&self) -> Ref<'_, PodsInfo> {
    self.rx.borrow()
  }

  pub fn borrow_and_update(&mut self) -> Ref<'_, PodsInfo> {
    self.rx.borrow_and_update()
  }

  pub async fn changed(&mut self) {
    let _ignored = self.rx.changed().await;
  }

  #[must_use]
  pub fn node_info(&self) -> Arc<NodeInfo> {
    self.parent.node_info.clone()
  }
}

//
// PodsInfoSingleton
//

// Singleton access for the pod info watcher.
pub struct PodsInfoSingleton {
  rx: watch::Receiver<PodsInfo>,
  node_info: Arc<NodeInfo>,
}

impl PodsInfoSingleton {
  #[must_use]
  pub const fn new(rx: watch::Receiver<PodsInfo>, node_info: Arc<NodeInfo>) -> Self {
    Self { rx, node_info }
  }

  #[must_use]
  pub fn make_owned(self: Arc<Self>) -> OwnedPodsInfoSingleton {
    let rx = self.rx.clone();
    OwnedPodsInfoSingleton { parent: self, rx }
  }

  pub fn node_name(kubernetes: &KubernetesBootstrapConfig) -> anyhow::Result<String> {
    env_or_inline_to_string(
      kubernetes
        .node_name
        .as_ref()
        .ok_or_else(missing_node_name_error)?,
    )
    .ok_or_else(missing_node_name_error)
  }

  pub async fn get(
    singleton_manager: Arc<SingletonManager>,
    k8s_config: KubernetesBootstrapConfig,
    shutdown: ComponentShutdownTriggerHandle,
    collector: Collector,
  ) -> anyhow::Result<Arc<Self>> {
    static HANDLE: OnceLock<SingletonHandle> = OnceLock::new();
    let handle = HANDLE.get_or_init(SingletonHandle::default);

    let node_name = Self::node_name(&k8s_config)?;
    let load_services = k8s_config.evaluate_services.unwrap_or(true);

    singleton_manager
      .get_or_init(handle, async {
        let (pods_info_tx, pods_info_rx) = watch::channel(PodsInfo::default());
        let node_info = NodeInfo::new(&node_name).await?;
        let service_cache = if load_services {
          Some(ServiceCache::new(
            k8s_config
              .services_cache_interval
              .as_ref()
              .map_or_else(|| 15.minutes(), bd_time::ProtoDurationExt::to_time_duration),
            Box::new(RealServiceFetcher),
          ))
        } else {
          None
        };

        watch_pods(
          k8s_config,
          node_info.clone(),
          pods_info_tx,
          service_cache,
          shutdown.make_shutdown(),
          &collector,
        )
        .await?;
        Ok::<_, anyhow::Error>(Arc::new(Self::new(pods_info_rx, node_info)))
      })
      .await
  }
}

pub mod container {
  use super::{
    InternalMetadataMatcher,
    InternalMetadataMatcherType,
    PodInfo,
    make_namespace_and_name,
  };
  use crate::k8s::services::ServiceCache;
  use crate::k8s::{NodeInfo, object_namespace};
  use crate::metadata::PodMetadata;
  use ahash::AHashMap;
  use k8s_openapi::api::core::v1::Pod;
  use kube::ResourceExt;
  use prometheus::IntCounter;
  use protobuf::Chars;
  use std::collections::{BTreeMap, HashMap};
  use std::net::IpAddr;
  use std::sync::Arc;

  #[derive(Default, Clone, Debug)]
  pub struct PodsInfo {
    by_name: AHashMap<String, Arc<PodInfo>>,
    by_ip: AHashMap<IpAddr, Arc<PodInfo>>,
  }

  impl PodsInfo {
    pub fn insert(&mut self, pod_info: PodInfo, insert_by_ip: bool) {
      let namespace_and_name = pod_info.namespace_and_name();
      let canonical_ip = pod_info.ip.to_canonical();
      log::info!("discovered pod '{namespace_and_name}' at ip '{canonical_ip}'");
      let pod_info = Arc::new(pod_info);
      self.by_name.insert(namespace_and_name, pod_info.clone());

      if insert_by_ip {
        let old = self.by_ip.insert(canonical_ip, pod_info);
        debug_assert!(old.is_none(), "duplicate ip '{canonical_ip}'");
      }
    }

    #[must_use]
    pub fn contains(&self, namespace: &str, pod_name: &str) -> bool {
      let namespace_and_name = make_namespace_and_name(namespace, pod_name);
      self.by_name.contains_key(&namespace_and_name)
    }

    pub fn remove(&mut self, namespace: &str, pod_name: &str) -> bool {
      let namespace_and_name = make_namespace_and_name(namespace, pod_name);
      if let Some(pod_info) = self.by_name.remove(&namespace_and_name) {
        log::info!("removing pod '{namespace_and_name}'");
        let canonical_ip = pod_info.ip.to_canonical();
        // In the host network case, make sure we are removing the correct pod from the IP map as
        // we may have ignored some on entry.
        if self
          .by_ip
          .get(&canonical_ip)
          .is_some_and(|p| p.namespace_and_name() == namespace_and_name)
        {
          log::info!("removing pod '{namespace_and_name}' from ip map");
          self.by_ip.remove(&canonical_ip);
        }

        return true;
      }
      false
    }

    #[must_use]
    pub fn by_name(&self, namespace: &str, pod_name: &str) -> Option<&Arc<PodInfo>> {
      self
        .by_name
        .get(&make_namespace_and_name(namespace, pod_name))
    }

    #[must_use]
    pub fn by_ip(&self, ip_addr: &IpAddr) -> Option<&Arc<PodInfo>> {
      let canonical_ip = ip_addr.to_canonical();
      self.by_ip.get(&canonical_ip)
    }

    pub fn pods(&self) -> impl Iterator<Item = (&String, &Arc<PodInfo>)> {
      self.by_name.iter()
    }

    pub(super) async fn apply_pod(
      &mut self,
      node_info: &NodeInfo,
      pod: &Pod,
      service_cache: Option<&mut ServiceCache>,
      pod_phases: &[Chars],
      host_network_pod_by_ip_filter: Option<&InternalMetadataMatcherType>,
      host_network_pod_by_ip_conflict: &IntCounter,
    ) -> bool {
      log::debug!("processing pod candidate");

      // TODO(snowp): Consider short circuiting if we know that nothing relevant changed.

      let Some(pod_name) = &pod.metadata.name else {
        log::trace!("skipping pod, no name");
        return false;
      };

      let Some(status) = &pod.status else {
        log::trace!("skipping pod {pod_name}, no status");
        return false;
      };

      let Some(pod_ip_string) = status.pod_ip.as_ref() else {
        log::trace!("skipping pod {pod_name}, no allocated IP");
        return false;
      };

      let Ok(pod_ip) = pod_ip_string.parse() else {
        log::trace!("skipping pod {pod_name}, failed to parse IP");
        return false;
      };

      let Some(phase) = &status.phase else {
        log::trace!("skipping pod {pod_name}, no phase");
        return false;
      };

      let mut container_ports = vec![];
      for container in pod
        .spec
        .as_ref()
        .map_or(&[][..], |spec| spec.containers.as_slice())
      {
        for port in container
          .ports
          .as_ref()
          .map_or(&[][..], |ports| ports.as_slice())
        {
          container_ports.push(super::ContainerPort {
            name: port.name.clone().unwrap_or_default(),
            port: port.container_port,
          });
        }
      }

      let namespace = object_namespace(&pod.metadata);

      // TODO(mattklein123): Potentially move this into the field selector query?
      if !pod_phases
        .iter()
        .any(|phase_char| phase.as_str() == phase_char.as_str())
      {
        if self.remove(namespace, pod_name) {
          log::trace!(
            "removing pod {}, no longer running",
            make_namespace_and_name(namespace, pod_name)
          );
          return true;
        }
        return false;
      }

      let mut pod_info = if self.contains(namespace, pod_name) {
        // If the pod is already in the map we don't consider further changes (beyond the phase
        // change above). Technically pod labels/annotations can change as well as service mappings
        // after the pod is running but we don't currently consider that case.
        return false;
      } else {
        PodInfo {
          services: HashMap::new(),
          namespace: namespace.to_string(),
          name: pod_name.to_string(),
          labels: pod.labels().clone(),
          annotations: pod.annotations().clone(),
          metadata: Arc::new(crate::metadata::Metadata::new(
            Some(node_info),
            Some(PodMetadata {
              namespace,
              pod_name,
              pod_ip: pod_ip_string,
              pod_labels: pod.labels(),
              pod_annotations: pod.annotations(),
              service: None,
            }),
            None,
          )),
          ip: pod_ip,
          ip_string: pod_ip_string.to_string(),
          container_ports,
        }
      };

      if let Some(service_cache) = service_cache {
        let services = service_cache.find_services(pod).await;
        for service in services {
          let previous = pod_info.services.insert(service.name.clone(), service);
          debug_assert!(previous.is_none());
        }
      }

      self.insert(
        pod_info,
        self.should_insert_by_ip(
          pod,
          pod_ip,
          host_network_pod_by_ip_filter,
          host_network_pod_by_ip_conflict,
        ),
      );
      true
    }

    fn metadata_matcher_matches(
      matcher: &InternalMetadataMatcher,
      metadata: &BTreeMap<String, String>,
    ) -> bool {
      metadata
        .get(matcher.name.as_str())
        .is_some_and(|v| matcher.value_regex.is_match(v))
    }

    fn should_insert_by_ip(
      &self,
      pod: &Pod,
      pod_ip: IpAddr,
      host_network_pod_by_ip_filter: Option<&InternalMetadataMatcherType>,
      host_network_pod_by_ip_conflict: &IntCounter,
    ) -> bool {
      let insert = (|| {
        if !pod
          .spec
          .as_ref()
          .and_then(|s| s.host_network)
          .unwrap_or(false)
        {
          return true;
        }

        let Some(host_network_pod_by_ip_filter) = host_network_pod_by_ip_filter else {
          return true;
        };

        match host_network_pod_by_ip_filter {
          InternalMetadataMatcherType::AnnotationMatcher(annotation_matcher) => {
            Self::metadata_matcher_matches(annotation_matcher, pod.annotations())
          },
          InternalMetadataMatcherType::LabelMatcher(label_matcher) => {
            Self::metadata_matcher_matches(label_matcher, pod.labels())
          },
        }
      })();

      if !insert {
        log::info!(
          "host network pod {}/{} with ip {} has been filtered from the IP map",
          object_namespace(&pod.metadata),
          pod.name_any(),
          pod_ip
        );
      }

      if insert && self.by_ip.contains_key(&pod_ip) {
        host_network_pod_by_ip_conflict.inc();
        log::warn!(
          "host network pod {}/{} with ip {} conflicts with existing pod {}",
          object_namespace(&pod.metadata),
          pod.name_any(),
          pod_ip,
          self.by_ip.get(&pod_ip).unwrap().namespace_and_name()
        );
        return false;
      }

      insert
    }
  }
}

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct ContainerPort {
  pub name: String,
  pub port: i32,
}

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct PodInfo {
  pub namespace: String,
  pub name: String,
  pub labels: BTreeMap<String, String>,
  pub annotations: BTreeMap<String, String>,
  pub services: HashMap<String, Arc<ServiceInfo>>,
  pub metadata: Arc<crate::metadata::Metadata>,
  pub ip: IpAddr,
  pub ip_string: String,
  pub container_ports: Vec<ContainerPort>,
}

impl PodInfo {
  #[must_use]
  pub fn namespace_and_name(&self) -> String {
    make_namespace_and_name(&self.namespace, &self.name)
  }
}

/// Performs a lookup of all eligible prom endpoints for the given k8s node and watches for
/// changes to this set. The initial state and updates are provided via the watch channel.
pub async fn watch_pods(
  config: KubernetesBootstrapConfig,
  node_info: Arc<NodeInfo>,
  update_tx: watch::Sender<PodsInfo>,
  service_cache: Option<ServiceCache>,
  shutdown: ComponentShutdown,
  collector: &Collector,
) -> anyhow::Result<()> {
  log::info!("starting pod watcher");
  let client = kube::Client::try_default().await?;
  let pod_api: Api<Pod> = kube::Api::all(client);
  let field_selector = Some(format!("spec.nodeName={}", node_info.name));
  let pods_info_cache = PodsInfoCache::new(node_info, update_tx, config, service_cache, collector)?;
  WatcherBase::create(
    "node pod watcher".to_string(),
    pod_api,
    field_selector,
    pods_info_cache,
    shutdown,
  )
  .await;

  Ok(())
}

//
// PodsInfoCache
//

struct PodsInfoCache {
  node_info: Arc<NodeInfo>,
  state: PodsInfo,
  update_tx: watch::Sender<PodsInfo>,
  pod_phases: Vec<Chars>,
  service_cache: Option<ServiceCache>,
  initializing_state: Option<PodsInfo>,
  host_network_pod_by_ip_filter: Option<InternalMetadataMatcherType>,
  host_network_pod_by_ip_conflict: IntCounter,
}
struct InternalMetadataMatcher {
  name: String,
  value_regex: Regex,
}
enum InternalMetadataMatcherType {
  AnnotationMatcher(InternalMetadataMatcher),
  LabelMatcher(InternalMetadataMatcher),
}

#[async_trait]
impl ResourceWatchCallbacks<Pod> for PodsInfoCache {
  async fn apply(&mut self, pod: Pod) {
    self.apply_pod(&pod).await;
  }

  async fn delete(&mut self, pod: Pod) {
    self.remove_pod(&pod);
  }

  async fn init_apply(&mut self, pod: Pod) {
    self
      .initializing_state
      .get_or_insert_with(PodsInfo::default)
      .apply_pod(
        &self.node_info,
        &pod,
        self.service_cache.as_mut(),
        &self.pod_phases,
        self.host_network_pod_by_ip_filter.as_ref(),
        &self.host_network_pod_by_ip_conflict,
      )
      .await;
  }

  async fn init_done(&mut self) {
    let new_state = self.initializing_state.take().unwrap_or_default();
    self.swap_state(new_state);
  }
}

impl PodsInfoCache {
  fn new(
    node_info: Arc<NodeInfo>,
    update_tx: watch::Sender<PodsInfo>,
    config: KubernetesBootstrapConfig,
    service_cache: Option<ServiceCache>,
    collector: &Collector,
  ) -> anyhow::Result<Self> {
    fn make_internal_metadata_matcher(
      matcher: &MetadataMatcher,
    ) -> anyhow::Result<InternalMetadataMatcher> {
      let value_regex = Regex::new(&matcher.value_regex)?;
      Ok(InternalMetadataMatcher {
        name: matcher.name.to_string(),
        value_regex,
      })
    }

    let host_network_pod_by_ip_filter: Option<InternalMetadataMatcherType> = config
      .host_network_pod_by_ip_filter
      .into_option()
      .map(|host_network_pod_by_ip_filter| {
        Ok::<_, anyhow::Error>(
          match host_network_pod_by_ip_filter.filter_type.expect("pgv") {
            Filter_type::AnnotationMatcher(annotation_matcher) => {
              InternalMetadataMatcherType::AnnotationMatcher(make_internal_metadata_matcher(
                &annotation_matcher,
              )?)
            },
            Filter_type::LabelMatcher(label_matcher) => InternalMetadataMatcherType::LabelMatcher(
              make_internal_metadata_matcher(&label_matcher)?,
            ),
          },
        )
      })
      .transpose()?;

    Ok(Self {
      node_info,
      state: PodsInfo::default(),
      update_tx,
      pod_phases: if config.pod_phases.is_empty() {
        vec!["Pending".into(), "Running".into()]
      } else {
        config.pod_phases
      },
      service_cache,
      initializing_state: None,
      host_network_pod_by_ip_filter,
      host_network_pod_by_ip_conflict: collector
        .scope("k8s_pods_info")
        .counter("host_network_pod_by_ip_conflict"),
    })
  }

  fn swap_state(&mut self, new_state: PodsInfo) {
    self.state = new_state;
    self.broadcast();
  }

  async fn apply_pod(&mut self, pod: &Pod) {
    if self
      .state
      .apply_pod(
        &self.node_info,
        pod,
        self.service_cache.as_mut(),
        &self.pod_phases,
        self.host_network_pod_by_ip_filter.as_ref(),
        &self.host_network_pod_by_ip_conflict,
      )
      .await
    {
      self.broadcast();
    }
  }

  fn remove_pod(&mut self, pod: &Pod) {
    let Some(name) = &pod.metadata.name else {
      return;
    };
    let namespace = object_namespace(&pod.metadata);

    if self.state.remove(namespace, name.as_str()) {
      self.broadcast();
    }
  }

  fn broadcast(&self) {
    log::info!(
      "broadcasting new pod info state with {} pods",
      self.state.pods().count()
    );
    log::debug!("broadcasting state: {:?}", self.state);
    let _ignored = self.update_tx.send(self.state.clone());
  }
}
