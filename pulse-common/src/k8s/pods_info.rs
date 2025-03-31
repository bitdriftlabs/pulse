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
use super::{NodeInfo, missing_node_name_error, object_namespace};
use crate::k8s::services::RealServiceFetcher;
use crate::proto::env_or_inline_to_string;
use crate::singleton::{SingletonHandle, SingletonManager};
use backoff::backoff::Backoff;
use backoff::{ExponentialBackoff, ExponentialBackoffBuilder};
use futures_util::future::BoxFuture;
use futures_util::{TryStreamExt, pin_mut};
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use kube::runtime::watcher::{self, ListSemantic};
use protobuf::Chars;
use pulse_protobuf::protos::pulse::config::bootstrap::v1::bootstrap::KubernetesBootstrapConfig;
use std::collections::{BTreeMap, HashMap};
use std::net::IpAddr;
use std::sync::{Arc, OnceLock};
use time::ext::{NumericalDuration, NumericalStdDuration};
use tokio::sync::oneshot;
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
              .map_or(15.minutes(), bd_time::ProtoDurationExt::to_time_duration),
            Box::new(RealServiceFetcher),
          ))
        } else {
          None
        };

        watch_pods(k8s_config, node_info.clone(), pods_info_tx, service_cache).await?;
        Ok::<_, anyhow::Error>(Arc::new(Self::new(pods_info_rx, node_info)))
      })
      .await
  }
}

pub mod container {
  use super::{PodInfo, make_namespace_and_name};
  use crate::k8s::services::ServiceCache;
  use crate::k8s::{NodeInfo, object_namespace};
  use crate::metadata::PodMetadata;
  use bd_log::warn_every;
  use k8s_openapi::api::core::v1::Pod;
  use kube::ResourceExt;
  use protobuf::Chars;
  use std::collections::HashMap;
  use std::net::IpAddr;
  use std::sync::Arc;
  use time::ext::NumericalDuration;

  // TODO(mattklein123): Consider using ahash or some faster map since IP lookup will be in the
  // fast path.
  #[derive(Default, Clone, Debug)]
  pub struct PodsInfo {
    by_name: HashMap<String, Arc<PodInfo>>,
    by_ip: HashMap<IpAddr, Arc<PodInfo>>,
  }

  impl PodsInfo {
    pub fn insert(&mut self, pod_info: PodInfo) {
      let namespace_and_name = pod_info.namespace_and_name();
      let canonical_ip = pod_info.ip.to_canonical();
      log::info!("discovered pod '{namespace_and_name}' at ip '{canonical_ip}'");
      let pod_info = Arc::new(pod_info);
      self.by_name.insert(namespace_and_name, pod_info.clone());
      self.by_ip.insert(canonical_ip, pod_info);
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
        if self.by_ip.remove(&canonical_ip).is_none() {
          warn_every!(
            1.minutes(),
            "no ip '{}' found for pod '{}'",
            canonical_ip,
            namespace_and_name
          );
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

    pub async fn apply_pod(
      &mut self,
      node_info: &NodeInfo,
      pod: &Pod,
      service_cache: Option<&mut ServiceCache>,
      pod_phases: &[Chars],
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
            node_info,
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

      self.insert(pod_info);
      true
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
  pub container_ports: Vec<ContainerPort>,
}

impl PodInfo {
  #[must_use]
  pub fn namespace_and_name(&self) -> String {
    make_namespace_and_name(&self.namespace, &self.name)
  }
}

fn make_k8s_backoff() -> ExponentialBackoff {
  // This matches the k8s client which says that it matches the Go client. This is done manually
  // as there appears to be a bug in the k8s client where it keeps resetting if the failure is
  // during initial sync.
  ExponentialBackoffBuilder::new()
    .with_initial_interval(800.std_milliseconds())
    .with_max_interval(30.std_seconds())
    .with_max_elapsed_time(None)
    .with_multiplier(2.0)
    .build()
}

/// Performs a lookup of all eligible prom endpoints for the given k8s node and watches for
/// changes to this set. The initial state and updates are provided via the watch channel.
pub async fn watch_pods(
  config: KubernetesBootstrapConfig,
  node_info: Arc<NodeInfo>,
  update_tx: tokio::sync::watch::Sender<PodsInfo>,
  mut service_cache: Option<ServiceCache>,
) -> anyhow::Result<()> {
  log::info!("starting pod watcher");
  let client = kube::Client::try_default().await?;
  let pod_api: Api<Pod> = kube::Api::all(client);

  // Note that we unset page size because the Rust library won't set resourceVersion=0 when
  // page size is set because apparently K8s ignores it. See:
  // https://github.com/kubernetes/kubernetes/issues/118394
  let watcher = watcher::watcher(
    pod_api,
    watcher::Config {
      field_selector: Some(format!("spec.nodeName={}", node_info.name)),
      list_semantic: ListSemantic::Any,
      page_size: None,
      ..Default::default()
    },
  );

  let mut pods_info_cache = PodsInfoCache::new(node_info, update_tx, config.pod_phases);
  let (initial_sync_tx, initial_sync_rx) = oneshot::channel();
  let mut backoff = make_k8s_backoff();

  tokio::spawn(async move {
    pin_mut!(watcher);
    let mut initial_state = None;
    let mut initial_sync_tx = Some(initial_sync_tx);
    loop {
      let Some(update) = process_resource_update(watcher.try_next().await, &mut backoff).await
      else {
        continue;
      };

      match update {
        watcher::Event::Apply(pod) => {
          pods_info_cache
            .apply_pod(&pod, service_cache.as_mut())
            .await;
        },
        watcher::Event::Delete(pod) => pods_info_cache.remove_pod(&pod),
        watcher::Event::Init => {
          log::info!("starting pod resync");
        },
        watcher::Event::InitApply(pod) => {
          initial_state
            .get_or_insert(PodsInfo::default())
            .apply_pod(
              &pods_info_cache.node_info,
              &pod,
              service_cache.as_mut(),
              &pods_info_cache.pod_phases,
            )
            .await;
        },
        watcher::Event::InitDone => {
          pods_info_cache.swap_state(initial_state.take().unwrap_or_default());
          log::info!("pod resync complete");
          if let Some(initial_sync_tx) = initial_sync_tx.take() {
            let _ignored = initial_sync_tx.send(());
          }
        },
      }
    }

    // TODO(snowp): We may do a periodic reconciliation pass to make sure that we don't drift out
    // of sync.
  });

  let _ignored = initial_sync_rx.await;
  log::info!("initial pod sync complete");

  Ok(())
}

async fn process_resource_update<T>(
  result: kube::runtime::watcher::Result<Option<kube::runtime::watcher::Event<T>>>,
  backoff: &mut ExponentialBackoff,
) -> Option<kube::runtime::watcher::Event<T>> {
  match result {
    Ok(Some(update)) => {
      if !matches!(update, watcher::Event::Init) {
        // The library will emit the Event::Init message in the case of a failure and the start of
        // resync. We do not want to reset in this case, but reset in all other cases.
        backoff.reset();
      }

      Some(update)
    },
    // TODO(snowp): Would this ever happen?
    Ok(None) => None,
    Err(e) => {
      log::warn!("Error watching pods, backing off: {e}");
      tokio::time::sleep(backoff.next_backoff().unwrap()).await;
      None
    },
  }
}

//
// PodsInfoCache
//

struct PodsInfoCache {
  node_info: Arc<NodeInfo>,
  state: PodsInfo,
  update_tx: tokio::sync::watch::Sender<PodsInfo>,
  pod_phases: Vec<Chars>,
}

impl PodsInfoCache {
  fn new(
    node_info: Arc<NodeInfo>,
    update_tx: tokio::sync::watch::Sender<PodsInfo>,
    pod_phases: Vec<Chars>,
  ) -> Self {
    Self {
      node_info,
      state: PodsInfo::default(),
      update_tx,
      pod_phases: if pod_phases.is_empty() {
        vec!["Pending".into(), "Running".into()]
      } else {
        pod_phases
      },
    }
  }

  fn swap_state(&mut self, new_state: PodsInfo) {
    self.state = new_state;
    self.broadcast();
  }

  async fn apply_pod(&mut self, pod: &Pod, service_cache: Option<&mut ServiceCache>) {
    if self
      .state
      .apply_pod(&self.node_info, pod, service_cache, &self.pod_phases)
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
