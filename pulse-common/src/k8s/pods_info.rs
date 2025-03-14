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
use super::missing_node_name_error;
use crate::proto::env_or_inline_to_string;
use crate::singleton::{SingletonHandle, SingletonManager};
use backoff::backoff::Backoff;
use backoff::{ExponentialBackoff, ExponentialBackoffBuilder};
use futures_util::future::BoxFuture;
use futures_util::{Stream, TryStreamExt, pin_mut};
use k8s_openapi::api::core::v1::{Pod, Service};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::core::ObjectMeta;
use kube::runtime::watcher;
use kube::runtime::watcher::Event;
use kube::{Api, ResourceExt};
use parking_lot::RwLock;
use protobuf::Chars;
use pulse_protobuf::protos::pulse::config::bootstrap::v1::bootstrap::KubernetesBootstrapConfig;
use std::collections::{BTreeMap, HashMap};
use std::net::IpAddr;
use std::sync::{Arc, OnceLock};
use time::ext::NumericalStdDuration;
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
  _parent: Arc<PodsInfoSingleton>,
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
}

//
// PodsInfoSingleton
//

// Singleton access for the pod info watcher.
pub struct PodsInfoSingleton {
  rx: watch::Receiver<PodsInfo>,
}

impl PodsInfoSingleton {
  #[must_use]
  pub const fn new(rx: watch::Receiver<PodsInfo>) -> Self {
    Self { rx }
  }

  #[must_use]
  pub fn make_owned(self: Arc<Self>) -> OwnedPodsInfoSingleton {
    let rx = self.rx.clone();
    OwnedPodsInfoSingleton { _parent: self, rx }
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

        watch_pods(
          k8s_config,
          node_name,
          pods_info_tx,
          if load_services {
            Some(service_watch_stream().await?)
          } else {
            None
          },
        )
        .await?;
        Ok::<_, anyhow::Error>(Arc::new(Self::new(pods_info_rx)))
      })
      .await
  }
}

pub mod container {
  use super::{PodInfo, ServiceMonitor, make_namespace_and_name};
  use crate::k8s::pods_info::object_namespace;
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
      log::info!(
        "discovered pod '{}' at ip '{}'",
        namespace_and_name,
        canonical_ip
      );
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
        log::info!("removing pod '{}'", namespace_and_name);
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

    pub fn apply_pod(
      &mut self,
      node_name: &str,
      pod: &Pod,
      service_cache: Option<&ServiceMonitor>,
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

      let Some(host_ip) = status.host_ip.as_ref() else {
        log::trace!("skipping pod {pod_name}, no host IP");
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
            namespace,
            pod_name,
            pod_ip_string,
            pod.labels(),
            pod.annotations(),
            None,
            node_name,
            host_ip,
            None,
          )),
          ip: pod_ip,
          container_ports,
          node_name: node_name.to_string(),
          node_ip: host_ip.clone(),
        }
      };

      if let Some(service_cache) = service_cache {
        let services = service_cache.find_services(pod);
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
  pub node_name: String,
  pub node_ip: String,
}

impl PodInfo {
  #[must_use]
  pub fn namespace_and_name(&self) -> String {
    make_namespace_and_name(&self.namespace, &self.name)
  }
}

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct ServiceInfo {
  pub name: String,
  pub annotations: BTreeMap<String, String>,
  pub selector: BTreeMap<String, String>,
  pub maybe_service_port: Option<i32>,
}

pub async fn service_watch_stream() -> anyhow::Result<
  impl Stream<Item = kube::runtime::watcher::Result<Event<Service>>> + Send + 'static,
> {
  let client = kube::Client::try_default().await?;
  let api = kube::Api::all(client);

  Ok(watcher(api, watcher::Config::default()))
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
  node: String,
  update_tx: tokio::sync::watch::Sender<PodsInfo>,
  watch_service_stream: Option<
    impl Stream<Item = kube::runtime::watcher::Result<Event<Service>>> + Send + 'static,
  >,
) -> kube::Result<()> {
  // First we need to track active services, as this is necessary in order to read the service
  // annotations for a given pod.
  let service_cache = if let Some(watch_service_stream) = watch_service_stream {
    let service_cache = Arc::new(ServiceMonitor::default());
    service_cache.monitor_services(watch_service_stream).await?;
    Some(service_cache)
  } else {
    None
  };

  log::info!("starting pod watcher");
  let client = kube::Client::try_default().await?;
  let pod_api: Api<Pod> = kube::Api::all(client);

  let watcher = watcher(
    pod_api,
    watcher::Config {
      field_selector: Some(format!("spec.nodeName={node}")),
      ..Default::default()
    },
  );

  let mut pods_info_cache = PodsInfoCache::new(node, update_tx, config.pod_phases);
  let (initial_sync_tx, initial_sync_rx) = oneshot::channel();
  let mut backoff = make_k8s_backoff();

  tokio::spawn(async move {
    pin_mut!(watcher);
    let mut initial_state = None;
    let mut initial_sync_tx = Some(initial_sync_tx);
    loop {
      let Some(update) = process_resource_update(watcher.try_next().await) else {
        tokio::time::sleep(backoff.next_backoff().unwrap()).await;
        continue;
      };

      if !matches!(update, watcher::Event::Init) {
        // The library will emit the Event::Init message in the case of a failure and the start of
        // resync. We do not want to reset in this case, but reset in all other cases.
        backoff.reset();
      }

      match update {
        watcher::Event::Apply(pod) => {
          pods_info_cache.apply_pod(&pod, service_cache.as_deref());
        },
        watcher::Event::Delete(pod) => pods_info_cache.remove_pod(&pod),
        watcher::Event::Init => {
          log::info!("starting pod resync");
        },
        watcher::Event::InitApply(pod) => {
          initial_state.get_or_insert(PodsInfo::default()).apply_pod(
            &pods_info_cache.node_name,
            &pod,
            service_cache.as_deref(),
            &pods_info_cache.pod_phases,
          );
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

fn process_resource_update<T>(
  result: kube::runtime::watcher::Result<Option<kube::runtime::watcher::Event<T>>>,
) -> Option<kube::runtime::watcher::Event<T>> {
  match result {
    Ok(Some(pod_update)) => Some(pod_update),
    // TODO(snowp): Would this ever happen?
    Ok(None) => None,
    Err(e) => {
      log::warn!("Error watching pods: {}", e);
      None
    },
  }
}

//
// PodsInfoCache
//

struct PodsInfoCache {
  node_name: String,
  state: PodsInfo,
  update_tx: tokio::sync::watch::Sender<PodsInfo>,
  pod_phases: Vec<Chars>,
}

impl PodsInfoCache {
  fn new(
    node_name: String,
    update_tx: tokio::sync::watch::Sender<PodsInfo>,
    pod_phases: Vec<Chars>,
  ) -> Self {
    Self {
      node_name,
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

  fn apply_pod(&mut self, pod: &Pod, service_cache: Option<&ServiceMonitor>) {
    if self
      .state
      .apply_pod(&self.node_name, pod, service_cache, &self.pod_phases)
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
    log::debug!("broadcasting state: {:?}", self.state);
    let _ignored = self.update_tx.send(self.state.clone());
  }
}

//
// ServiceCache
//

#[derive(Default)]
struct ServiceCache {
  services_by_namespace: HashMap<String, HashMap<String, Arc<ServiceInfo>>>,
}

impl ServiceCache {
  fn apply_service(&mut self, service: Service) {
    let maybe_service_port = service.spec.as_ref().and_then(|spec| {
      Some(match spec.ports.as_ref()?.first()?.target_port.as_ref()? {
        IntOrString::Int(i) => *i,
        IntOrString::String(s) => s.parse().ok()?,
      })
    });

    self
      .services_by_namespace
      .entry(object_namespace(&service.metadata).to_string())
      .or_default()
      .insert(
        service.metadata.name.as_deref().unwrap().to_string(),
        Arc::new(ServiceInfo {
          name: service.metadata.name.clone().unwrap_or_default(),
          annotations: service.annotations().clone(),
          selector: service
            .spec
            .and_then(|spec| spec.selector)
            .unwrap_or_default(),
          maybe_service_port,
        }),
      );
  }

  fn remove_service(&mut self, service: Service) {
    let namespace = object_namespace(&service.metadata).to_string();

    let entry = self
      .services_by_namespace
      .entry(namespace.to_string())
      .or_default();
    entry.remove_entry(&service.metadata.name.unwrap());

    if entry.is_empty() {
      self.services_by_namespace.remove_entry(&namespace);
    }
  }
}

//
// ServiceMonitor
//

/// Cache of service entites per namespace. We use this to resolve the service annotations for pods
/// found on the local node. For now we watch all services, but this could be optimized to only
/// look for services in the namespaces we care about.
#[derive(Default)]
pub struct ServiceMonitor {
  cache: RwLock<ServiceCache>,
}

type ServiceResult = kube::runtime::watcher::Result<Event<Service>>;

impl ServiceMonitor {
  /// Syncs the current set of services to the internal cache and sets up a watch to watch for
  /// any changes to the set of services.
  async fn monitor_services(
    self: &Arc<Self>,
    watch_stream: impl Stream<Item = ServiceResult> + Send + 'static,
  ) -> kube::Result<()> {
    let watcher = watch_stream;
    let self_clone = self.clone();
    let (initial_sync_tx, initial_sync_rx) = oneshot::channel();
    let mut backoff = make_k8s_backoff();
    tokio::spawn(async move {
      pin_mut!(watcher);
      let mut initial_state = None;
      let mut initial_sync_tx = Some(initial_sync_tx);
      loop {
        let Some(service_update) = process_resource_update(watcher.try_next().await) else {
          tokio::time::sleep(backoff.next_backoff().unwrap()).await;
          continue;
        };

        if !matches!(service_update, watcher::Event::Init) {
          // The library will emit the Event::Init message in the case of a failure and the start of
          // resync. We do not want to reset in this case, but reset in all other cases.
          backoff.reset();
        }

        match service_update {
          watcher::Event::Apply(service) => {
            self_clone.cache.write().apply_service(service);
          },
          watcher::Event::Delete(service) => {
            self_clone.cache.write().remove_service(service);
          },
          watcher::Event::Init => {
            log::info!("starting service resync");
          },
          watcher::Event::InitApply(service) => {
            initial_state
              .get_or_insert(ServiceCache::default())
              .apply_service(service);
          },
          watcher::Event::InitDone => {
            *self_clone.cache.write() = initial_state.take().unwrap_or_default();
            log::info!("service resync complete");
            if let Some(initial_sync_tx) = initial_sync_tx.take() {
              let _ignored = initial_sync_tx.send(());
            }
          },
        };
      }
    });

    let _ignored = initial_sync_rx.await;
    log::info!("initial service sync complete");

    // TODO(snowp): We may do a periodic reconciliation pass to make sure that we don't drift out
    // of sync.

    Ok(())
  }

  /// Attempts to resolve an active service for the provided pod. This is done by attempting to
  /// match the pod against all active services in the namespace of the pod.
  fn find_services(&self, pod: &Pod) -> Vec<Arc<ServiceInfo>> {
    let cache = self.cache.read();

    let Some(services) = cache
      .services_by_namespace
      .get(object_namespace(&pod.metadata))
    else {
      return vec![];
    };

    // TODO(snowp): There is a use case for collecting metrics for each active service, so this
    // should return all matching services instead.
    services
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
}

/// Returns the namespace for the provided object.
fn object_namespace(meta: &ObjectMeta) -> &str {
  meta.namespace.as_deref().unwrap_or("default")
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
