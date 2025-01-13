// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./mod_test.rs"]
mod mod_test;

use self::config::{check_routes, PipelineComponent, PipelineComponentType, PipelineRouteType};
use self::inflow::{DynamicPipelineInflow, InflowFactoryContext, PipelineInflow};
use self::metric_cache::MetricCache;
use self::outflow::{DynamicPipelineOutflow, OutflowFactoryContext, OutflowStats, PipelineOutflow};
use self::processor::{DynamicPipelineProcessor, PipelineProcessor, ProcessorFactoryContext};
use self::time::RealTimeProvider;
use crate::admin::server::Admin;
use crate::protos::metric::ParsedMetric;
use anyhow::anyhow;
use async_trait::async_trait;
use bd_proto_util::proto::hash_message;
use bd_server_stats::stats::Scope;
use bd_shutdown::{ComponentShutdown, ComponentShutdownTrigger};
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use futures_util::future::BoxFuture;
use itertools::Itertools;
use mockall::automock;
use parking_lot::RwLock;
use prometheus::IntCounter;
use protobuf::{Chars, MessageFull};
use pulse_common::bind_resolver::BindResolver;
use pulse_common::k8s::pods_info::K8sWatchFactory;
use pulse_common::singleton::SingletonManager;
use pulse_protobuf::protos::pulse::config::bootstrap::v1::bootstrap::{
  KubernetesBootstrapConfig,
  PipelineConfig,
};
use pulse_protobuf::protos::pulse::config::inflow::v1::inflow::InflowConfig;
use pulse_protobuf::protos::pulse::config::outflow::v1::outflow::OutflowConfig;
use pulse_protobuf::protos::pulse::config::processor::v1::processor::ProcessorConfig;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use topological_sort::TopologicalSort;
use xxhash_rust::xxh64::Xxh64;

pub mod config;
pub mod inflow;
pub mod metric_cache;
pub mod outflow;
pub mod processor;
pub mod time;

//
// MapEntry
//

/// A wrapper to attach a Trigger to each ``HashMap`` entry. Dropping an entry
/// will trigger the associated watch, initiating shutdown of the associated
/// component.
pub struct MapEntry<T: ?Sized> {
  trigger: ComponentShutdownTrigger,
  item: Arc<T>,
}

impl<T: ?Sized> MapEntry<T> {
  pub const fn new(item: Arc<T>, trigger: ComponentShutdownTrigger) -> Self {
    Self { trigger, item }
  }

  pub async fn shutdown(self, component_type: &str, name: String) {
    log::info!("shutting down \"{}\" {}", name, component_type);
    self.trigger.shutdown().await;
    debug_assert_eq!(1, Arc::strong_count(&self.item));
    log::info!("successfully shut down \"{}\" {}", name, component_type);
  }

  #[must_use]
  pub const fn item(&self) -> &Arc<T> {
    &self.item
  }
}

//
// PipelineItemWithDispatcher
//

struct PipelineItemWithDispatcher<T: ?Sized> {
  dispatcher: Arc<PipelineDispatcher>,
  item: MapEntry<T>,
  config_hash: u64,
}

//
// PipelineItem
//

struct PipelineItem<T: ?Sized> {
  item: MapEntry<T>,
  config_hash: u64,
}

//
// PipelineState
//

#[derive(Default)]
struct PipelineState {
  inflows: HashMap<String, PipelineItemWithDispatcher<dyn PipelineInflow + Send + Sync>>,
  processors: HashMap<String, PipelineItemWithDispatcher<dyn PipelineProcessor + Send + Sync>>,
  outflows: HashMap<String, PipelineItem<dyn PipelineOutflow + Send + Sync>>,
}

//
// PipelineDispatch
//

#[automock]
#[async_trait]
pub trait PipelineDispatch: Send + Sync {
  async fn send(&self, samples: Vec<ParsedMetric>);
  async fn send_alt(&self, samples: Vec<ParsedMetric>);
}

//
// ResolvedRouteTarget
//

#[derive(Clone)]
enum ResolvedRouteTarget {
  Processor(DynamicPipelineProcessor),
  Outflow(DynamicPipelineOutflow),
}

//
// ResolvedRoute
//

#[derive(Clone)]
struct ResolvedRoute {
  routed: IntCounter,
  target: ResolvedRouteTarget,
}

/// A PipelineRoute specifies a destination in a [PipelineConfig].
///
/// For example:
///
/// "outflow:wavefrontproxy"
struct PipelineRouteInner {
  route_type: PipelineRouteType,
  route_to: String,
  resolved: RwLock<Option<ResolvedRoute>>,
}
#[derive(Clone)]
pub struct PipelineRoute {
  inner: Arc<PipelineRouteInner>,
}

impl PipelineRoute {
  #[must_use]
  pub fn new(route_type: PipelineRouteType, route_to: String) -> Self {
    Self {
      inner: Arc::new(PipelineRouteInner {
        route_type,
        route_to,
        resolved: RwLock::default(),
      }),
    }
  }

  fn from_config_string(route: &Chars) -> anyhow::Result<Self> {
    let parts: Vec<&str> = route.split(':').collect();
    if let [ty, to] = &parts[..] {
      Ok(Self {
        inner: Arc::new(PipelineRouteInner {
          route_type: (*ty).try_into()?,
          route_to: (*to).into(),
          resolved: RwLock::default(),
        }),
      })
    } else {
      Err(anyhow!("malformed route: {route}"))
    }
  }

  pub fn from_config_strings(route: &[Chars]) -> anyhow::Result<Vec<Self>> {
    route.iter().map(Self::from_config_string).try_collect()
  }
}

impl fmt::Display for PipelineRoute {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}:{}", self.inner.route_type, self.inner.route_to)
  }
}

//
// PipelineDispatcher
//

/// The PipelineDispatcher is responsible for routing messages through the pipeline.
struct PipelineDispatcher {
  routes: Vec<PipelineRoute>,
  alt_routes: Vec<PipelineRoute>,
}

impl PipelineDispatcher {
  fn new(routes: &[Chars], alt_routes: &[Chars]) -> anyhow::Result<Self> {
    Ok(Self {
      routes: PipelineRoute::from_config_strings(routes)?,
      alt_routes: PipelineRoute::from_config_strings(alt_routes)?,
    })
  }

  /// Route to a single destination.
  async fn send_one(samples: Vec<ParsedMetric>, dest: &PipelineRoute) {
    let resolved = dest.inner.resolved.read().as_ref().unwrap().clone();
    resolved.routed.inc_by(samples.len().try_into().unwrap());
    match &resolved.target {
      ResolvedRouteTarget::Processor(processor) => {
        processor.clone().recv_samples(samples).await;
      },
      ResolvedRouteTarget::Outflow(outflow) => {
        outflow.recv_samples(samples).await;
      },
    }
  }

  /// Route to multiple destinations concurrently.
  async fn send_core(samples: Vec<ParsedMetric>, routes: &[PipelineRoute]) {
    if samples.is_empty() {
      return;
    }

    let mut futures = FuturesUnordered::new();
    // Split off last to avoid unnecessary clones
    if let Some((last_dest, dests)) = routes.split_last() {
      for dest in dests {
        futures.push(Self::send_one(samples.clone(), dest));
      }
      futures.push(Self::send_one(samples, last_dest));
    }
    while (futures.next().await).is_some() {}
  }

  fn dependencies(&self) -> Vec<PipelineRoute> {
    let mut deps = self.routes.clone();
    deps.extend(self.alt_routes.clone());
    deps
  }
}

#[async_trait]
impl PipelineDispatch for PipelineDispatcher {
  async fn send(&self, samples: Vec<ParsedMetric>) {
    Self::send_core(samples, &self.routes).await;
  }

  async fn send_alt(&self, samples: Vec<ParsedMetric>) {
    Self::send_core(samples, &self.alt_routes).await;
  }
}

//
// ItemFactory
//

// Abstraction over creating pipeline components to allow for easier testing of dynamic config.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait ItemFactory: Send + Sync {
  async fn to_inflow(
    &self,
    config: InflowConfig,
    context: InflowFactoryContext,
  ) -> anyhow::Result<DynamicPipelineInflow>;

  async fn to_processor(
    &self,
    config: ProcessorConfig,
    context: ProcessorFactoryContext,
  ) -> anyhow::Result<DynamicPipelineProcessor>;

  async fn to_outflow(
    &self,
    config: OutflowConfig,
    context: OutflowFactoryContext,
  ) -> anyhow::Result<DynamicPipelineOutflow>;
}

//
// RealItemFactory
//

// Item factory that calls through to the real implementation.
pub struct RealItemFactory {}

#[async_trait]
impl ItemFactory for RealItemFactory {
  async fn to_inflow(
    &self,
    config: InflowConfig,
    context: InflowFactoryContext,
  ) -> anyhow::Result<DynamicPipelineInflow> {
    inflow::to_inflow(config, context).await
  }

  async fn to_processor(
    &self,
    config: ProcessorConfig,
    context: ProcessorFactoryContext,
  ) -> anyhow::Result<DynamicPipelineProcessor> {
    processor::to_processor(config, context).await
  }

  async fn to_outflow(
    &self,
    config: OutflowConfig,
    context: OutflowFactoryContext,
  ) -> anyhow::Result<DynamicPipelineOutflow> {
    outflow::to_outflow(config, context).await
  }
}

//
// MetricPipeline
//

pub struct MetricPipeline {
  state: TokioMutex<PipelineState>,
  factory: Arc<dyn ItemFactory>,
  scope: Scope,
  k8s_config: KubernetesBootstrapConfig,
  k8s_watch_factory: K8sWatchFactory,
  singleton_manager: Arc<SingletonManager>,
  admin: Arc<dyn Admin>,
  bind_resolver: Arc<dyn BindResolver>,
  metric_cache: Arc<MetricCache>,
}

impl MetricPipeline {
  pub async fn new_from_config(
    factory: Arc<dyn ItemFactory>,
    scope: Scope,
    k8s_config: KubernetesBootstrapConfig,
    k8s_watch_factory: K8sWatchFactory,
    pipeline_config: PipelineConfig,
    singleton_manager: Arc<SingletonManager>,
    admin: Arc<dyn Admin>,
    bind_resolver: Arc<dyn BindResolver>,
  ) -> anyhow::Result<Self> {
    check_routes(&pipeline_config)?;

    let metric_cache = MetricCache::new(&scope, pipeline_config.advanced.max_cached_metrics);
    let mut state = PipelineState::default();
    if let Err(e) = Self::new_from_config_inner(
      &mut state,
      &metric_cache,
      &factory,
      &scope,
      &k8s_config,
      &k8s_watch_factory,
      pipeline_config,
      &singleton_manager,
      &admin,
      &bind_resolver,
    )
    .await
    {
      log::error!("error during pipeline construction, shutting down: {e}");
      Self::shutdown_pipeline_state(&mut state).await;
      return Err(e);
    }

    Ok(Self {
      state: TokioMutex::new(state),
      factory,
      scope,
      k8s_config,
      k8s_watch_factory,
      singleton_manager,
      admin,
      bind_resolver,
      metric_cache,
    })
  }

  async fn new_from_config_inner(
    state: &mut PipelineState,
    metric_cache: &Arc<MetricCache>,
    factory: &Arc<dyn ItemFactory>,
    scope: &Scope,
    k8s_config: &KubernetesBootstrapConfig,
    k8s_watch_factory: &K8sWatchFactory,
    pipeline_config: PipelineConfig,
    singleton_manager: &Arc<SingletonManager>,
    admin: &Arc<dyn Admin>,
    bind_resolver: &Arc<dyn BindResolver>,
  ) -> anyhow::Result<()> {
    for (name, config) in pipeline_config.outflows {
      Self::add_outflow(factory.as_ref(), scope, state, name.into(), config).await?;
    }
    for (name, config) in pipeline_config.processors {
      Self::add_processor(
        factory.as_ref(),
        scope,
        state,
        metric_cache.clone(),
        name.into(),
        config,
        singleton_manager.clone(),
        admin.clone(),
        bind_resolver.clone(),
        k8s_watch_factory.clone(),
      )
      .await?;
    }
    for (name, config) in pipeline_config.inflows {
      Self::add_inflow(
        factory.as_ref(),
        scope,
        state,
        name.into(),
        k8s_config.clone(),
        k8s_watch_factory.clone(),
        config,
        bind_resolver.clone(),
        singleton_manager.clone(),
      )
      .await?;
    }

    // Now that everything is constructed we can fully resolve all of the routes.
    for (name, inflow) in &state.inflows {
      Self::resolve_routes(
        name,
        PipelineComponentType::Inflow,
        &inflow.dispatcher.dependencies(),
        state,
        scope,
      );
    }
    for (name, processor) in &state.processors {
      Self::resolve_routes(
        name,
        PipelineComponentType::Processor,
        &processor.dispatcher.dependencies(),
        state,
        scope,
      );
    }

    Ok(())
  }

  // Updating an existing configuration in place.
  pub async fn update_config(&self, config: PipelineConfig) -> anyhow::Result<()> {
    check_routes(&config)?;

    let mut old_state = self.state.lock().await;
    let mut new_state = PipelineState::default();
    log::debug!("beginning config update");

    // Step one is to make any new components. Try to fail early without disturbing the existing
    // pipeline.
    let mut outflows_to_move = HashSet::new();
    let mut processors_to_move = HashSet::new();
    let mut inflows_to_move = HashSet::new();
    for (name, config) in config.outflows {
      let new_config_hash = Self::hash_config(&config);
      if old_state
        .outflows
        .get(name.as_str())
        .is_some_and(|o| o.config_hash == new_config_hash)
      {
        log::debug!("using existing outflow '{name}' with config hash '{new_config_hash}'");
        outflows_to_move.insert(name);
      } else {
        Self::add_outflow(
          self.factory.as_ref(),
          &self.scope,
          &mut new_state,
          name.into(),
          config,
        )
        .await?;
      }
    }
    for (name, config) in config.processors {
      let new_config_hash = Self::hash_config(&config);
      if old_state
        .processors
        .get(name.as_str())
        .is_some_and(|p| p.config_hash == new_config_hash)
      {
        log::debug!("using existing processor '{name}' with config hash '{new_config_hash}'");
        processors_to_move.insert(name);
      } else {
        Self::add_processor(
          self.factory.as_ref(),
          &self.scope,
          &mut new_state,
          self.metric_cache.clone(),
          name.into(),
          config,
          self.singleton_manager.clone(),
          self.admin.clone(),
          self.bind_resolver.clone(),
          self.k8s_watch_factory.clone(),
        )
        .await?;
      }
    }
    for (name, config) in config.inflows {
      let new_config_hash = Self::hash_config(&config);
      if old_state
        .inflows
        .get(name.as_str())
        .is_some_and(|p| p.config_hash == new_config_hash)
      {
        log::debug!("using existing inflow '{name}' with config hash '{new_config_hash}'");
        inflows_to_move.insert(name);
      } else {
        Self::add_inflow(
          self.factory.as_ref(),
          &self.scope,
          &mut new_state,
          name.into(),
          self.k8s_config.clone(),
          self.k8s_watch_factory.clone(),
          config,
          self.bind_resolver.clone(),
          self.singleton_manager.clone(),
        )
        .await?;
      }
    }

    // Now proceed to move components over. We go back to front, starting as needed and then
    // resolving routes. At this point we are beyond the point of no return and cannot easily
    // roll back. First outflows get moved, as they do not require starting and do not have any
    // routes to resolve.
    log::debug!("migrating old outflows");
    for outflow in outflows_to_move {
      new_state.outflows.insert(
        outflow.clone().into(),
        old_state.outflows.remove(outflow.as_str()).unwrap(),
      );
    }

    // New processors have to be started, then old processors moved, then everything (re)resolved
    // to the new set of outflows.
    log::debug!("migrating old processors");
    let new_processors: Vec<_> = new_state.processors.keys().cloned().collect();
    for processor in processors_to_move {
      new_state.processors.insert(
        processor.clone().into(),
        old_state.processors.remove(processor.as_str()).unwrap(),
      );
    }
    for (name, processor) in &new_state.processors {
      Self::resolve_routes(
        name,
        PipelineComponentType::Processor,
        &processor.dispatcher.dependencies(),
        &new_state,
        &self.scope,
      );
    }
    for new_processor in new_processors {
      new_state.processors[&new_processor]
        .item
        .item()
        .clone()
        .start()
        .await;
    }

    // Now the same for inflows.
    log::debug!("migrating old inflows");
    let new_inflows: Vec<_> = new_state.inflows.keys().cloned().collect();
    for inflow in inflows_to_move {
      new_state.inflows.insert(
        inflow.clone().into(),
        old_state.inflows.remove(inflow.as_str()).unwrap(),
      );
    }
    for (name, inflow) in &new_state.inflows {
      Self::resolve_routes(
        name,
        PipelineComponentType::Inflow,
        &inflow.dispatcher.dependencies(),
        &new_state,
        &self.scope,
      );
    }
    for new_inflow in new_inflows {
      new_state.inflows[&new_inflow]
        .item
        .item()
        .clone()
        .start()
        .await;
    }

    // Now shutdown any remaining components that are no longer needed.
    Self::shutdown_pipeline_state(&mut old_state).await;
    debug_assert!(
      old_state.inflows.is_empty()
        && old_state.processors.is_empty()
        && old_state.outflows.is_empty()
    );
    *old_state = new_state;

    log::debug!("config update finished");
    Ok(())
  }

  fn make_route_counter(
    stats: &Scope,
    source_type: PipelineComponentType,
    source_name: &str,
    dependency: &PipelineRoute,
  ) -> IntCounter {
    stats.counter_with_labels(
      "messages_routed",
      HashMap::from([
        ("src_type".to_string(), source_type.to_string()),
        ("src".to_string(), source_name.to_string()),
        (
          "dest_type".to_string(),
          dependency.inner.route_type.to_string(),
        ),
        ("dest".to_string(), dependency.inner.route_to.clone()),
      ]),
    )
  }

  fn resolve_routes(
    source_name: &str,
    source_type: PipelineComponentType,
    dependencies: &[PipelineRoute],
    state: &PipelineState,
    stats: &Scope,
  ) {
    for dependency in dependencies {
      match dependency.inner.route_type {
        PipelineRouteType::Processor => {
          *dependency.inner.resolved.write() = Some(ResolvedRoute {
            routed: Self::make_route_counter(stats, source_type, source_name, dependency),
            target: ResolvedRouteTarget::Processor(
              state
                .processors
                .get(&dependency.inner.route_to)
                .unwrap()
                .item
                .item()
                .clone(),
            ),
          });
        },
        PipelineRouteType::Outflow => {
          *dependency.inner.resolved.write() = Some(ResolvedRoute {
            routed: Self::make_route_counter(stats, source_type, source_name, dependency),
            target: ResolvedRouteTarget::Outflow(
              state
                .outflows
                .get(&dependency.inner.route_to)
                .unwrap()
                .item
                .item()
                .clone(),
            ),
          });
        },
      }
    }
  }

  pub async fn start(&self) {
    let state = self.state.lock().await;
    for processor in state.processors.values() {
      processor.item.item().clone().start().await;
    }

    for inflow in state.inflows.values() {
      inflow.item.item().clone().start().await;
    }
  }

  fn hash_config<T: MessageFull>(message: &T) -> u64 {
    let mut hasher = Xxh64::default();
    hash_message(&mut hasher, message);
    hasher.digest()
  }

  async fn add_inflow(
    factory: &dyn ItemFactory,
    scope: &Scope,
    state: &mut PipelineState,
    name: String,
    k8s_config: KubernetesBootstrapConfig,
    k8s_watch_factory: K8sWatchFactory,
    inflow_config: InflowConfig,
    bind_resolver: Arc<dyn BindResolver>,
    singleton_manager: Arc<SingletonManager>,
  ) -> anyhow::Result<()> {
    let scope = scope.scope("inflow").scope(name.as_str());
    let shutdown_trigger = ComponentShutdownTrigger::default();
    let dispatcher = Arc::new(PipelineDispatcher::new(&inflow_config.routes, &[])?);
    let config_hash = Self::hash_config(&inflow_config);
    let inflow = factory
      .to_inflow(
        inflow_config,
        InflowFactoryContext {
          k8s_config,
          k8s_watch_factory,
          name: name.clone(),
          scope,
          dispatcher: dispatcher.clone(),
          shutdown_trigger_handle: shutdown_trigger.make_handle(),
          bind_resolver,
          singleton_manager,
        },
      )
      .await?;
    log::debug!("adding inflow '{name}' with config hash '{config_hash}'");
    state.inflows.insert(
      name,
      PipelineItemWithDispatcher {
        dispatcher,
        item: MapEntry::new(inflow, shutdown_trigger),
        config_hash,
      },
    );
    Ok(())
  }

  async fn add_processor(
    factory: &dyn ItemFactory,
    scope: &Scope,
    state: &mut PipelineState,
    metric_cache: Arc<MetricCache>,
    name: String,
    processor_config: ProcessorConfig,
    singleton_manager: Arc<SingletonManager>,
    admin: Arc<dyn Admin>,
    bind_resolver: Arc<dyn BindResolver>,
    k8s_watch_factory: K8sWatchFactory,
  ) -> anyhow::Result<()> {
    let scope = scope.scope("processor").scope(name.as_str());
    let shutdown_trigger = ComponentShutdownTrigger::default();
    let dispatcher = Arc::new(PipelineDispatcher::new(
      &processor_config.routes,
      &processor_config.alt_routes,
    )?);
    let config_hash = Self::hash_config(&processor_config);
    let processor = factory
      .to_processor(
        processor_config,
        ProcessorFactoryContext {
          k8s_watch_factory,
          name: name.clone(),
          scope,
          metric_cache,
          dispatcher: dispatcher.clone(),
          shutdown_trigger_handle: shutdown_trigger.make_handle(),
          singleton_manager,
          admin,
          bind_resolver,
          time_provider: Box::new(RealTimeProvider {}),
        },
      )
      .await?;
    log::debug!("adding processor '{name}' with config hash '{config_hash}'");
    state.processors.insert(
      name,
      PipelineItemWithDispatcher {
        dispatcher,
        item: MapEntry::new(processor, shutdown_trigger),
        config_hash,
      },
    );
    Ok(())
  }

  async fn add_outflow(
    factory: &dyn ItemFactory,
    scope: &Scope,
    state: &mut PipelineState,
    name: String,
    outflow_config: OutflowConfig,
  ) -> anyhow::Result<()> {
    let stats = OutflowStats::new(scope, name.as_str());
    let shutdown_trigger = ComponentShutdownTrigger::default();
    let config_hash = Self::hash_config(&outflow_config);
    let outflow = factory
      .to_outflow(
        outflow_config,
        OutflowFactoryContext {
          name: name.clone(),
          stats,
          shutdown_trigger_handle: shutdown_trigger.make_handle(),
        },
      )
      .await?;
    log::debug!("adding outflow '{name}' with config hash '{config_hash}'");
    state.outflows.insert(
      name,
      PipelineItem {
        item: MapEntry::new(outflow, shutdown_trigger),
        config_hash,
      },
    );
    Ok(())
  }

  async fn topological_sort(state: &PipelineState) -> TopologicalSort<PipelineComponent> {
    let mut ts = TopologicalSort::new();
    for (inflow_name, inflow) in &state.inflows {
      let inflow_component = PipelineComponent {
        name: inflow_name.clone(),
        component_type: PipelineComponentType::Inflow,
      };
      for dependency in inflow.dispatcher.dependencies() {
        let dependent_component: PipelineComponent = dependency.into();
        ts.add_dependency(inflow_component.clone(), dependent_component);
      }
    }
    for (processor_name, processor) in &state.processors {
      let processor_component = PipelineComponent {
        name: processor_name.clone(),
        component_type: PipelineComponentType::Processor,
      };
      for dependency in processor.dispatcher.dependencies() {
        let dependent_component: PipelineComponent = dependency.into();
        ts.add_dependency(processor_component.clone(), dependent_component);
      }
    }
    // In the update case we may have dangling outflows so we need to explicitly add all outflows
    // if they haven't already been added.
    for outflow_name in state.outflows.keys() {
      let outflow_component = PipelineComponent {
        name: outflow_name.clone(),
        component_type: PipelineComponentType::Outflow,
      };
      ts.insert(outflow_component);
    }
    ts
  }

  async fn shutdown_pipeline_state(state: &mut PipelineState) {
    let mut ts = Self::topological_sort(state).await;
    loop {
      if ts.is_empty() {
        return;
      }

      let components = ts.pop_all();
      let mut futures: FuturesUnordered<BoxFuture<'_, _>> = FuturesUnordered::default();
      for component in components {
        let name = component.name;
        match component.component_type {
          PipelineComponentType::Inflow => {
            if let Some(inflow) = state.inflows.remove(name.as_str()) {
              futures.push(Box::pin(inflow.item.shutdown("inflow", name)));
            }
          },
          PipelineComponentType::Processor => {
            if let Some(processor) = state.processors.remove(name.as_str()) {
              futures.push(Box::pin(processor.item.shutdown("processor", name)));
            }
          },
          PipelineComponentType::Outflow => {
            if let Some(outflow) = state.outflows.remove(name.as_str()) {
              futures.push(Box::pin(outflow.item.shutdown("outflow", name)));
            }
          },
        }
        while (futures.next().await).is_some() {}
      }
    }
  }

  /// Shutdown works by dropping pipeline components in a topological order.
  pub async fn shutdown(&self) {
    let mut state = self.state.lock().await;
    Self::shutdown_pipeline_state(&mut state).await;
  }
}
