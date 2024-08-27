// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

pub mod config;
pub mod metadata;

#[cfg(test)]
mod test;

use anyhow::{anyhow, bail};
use bd_runtime_config::loader::{self, WatchedFileLoader};
use bd_server_stats::stats::{Collector, Scope};
use bd_shutdown::ComponentShutdownTrigger;
use futures::{Future, FutureExt};
use log::info;
use prometheus::IntCounter;
use pulse_common::bind_resolver::BindResolver;
use pulse_common::k8s::pods_info::PodsInfoSingleton;
use pulse_common::proto::{
  env_or_inline_to_string,
  yaml_value_to_proto,
  ProtoDurationToStdDuration,
};
use pulse_common::singleton::SingletonManager;
use pulse_metrics::admin::server::{AdminState, MetaStatsEmitter};
use pulse_metrics::admin::stats::StatsProvider;
use pulse_metrics::pipeline::{MetricPipeline, RealItemFactory};
use pulse_protobuf::protos::pulse::config::bootstrap::v1::bootstrap::config::Pipeline_type;
use pulse_protobuf::protos::pulse::config::bootstrap::v1::bootstrap::Config;
use regex::Regex;
use std::sync::Arc;
use std::time::Duration;
use time::ext::NumericalDuration;

#[cfg(test)]
#[ctor::ctor]
fn test_global_init() {
  use pulse_common::global_initialize;

  global_initialize();
}

//
// ServerHooks
//

#[async_trait::async_trait]
pub trait ServerHooks {
  async fn server_started(&self, collector: Collector);
}

//
// ConfigStats
//

struct ConfigStats {
  updated: IntCounter,
  failed_update: IntCounter,
}

impl ConfigStats {
  fn new(scope: &Scope) -> Self {
    let scope = scope.scope("config");
    Self {
      updated: scope.counter("updated"),
      failed_update: scope.counter("failed_update"),
    }
  }
}

pub fn build_stats_provider(config: &Config) -> anyhow::Result<StatsProvider> {
  let meta_stats = &config.meta_stats;
  let meta_tags = meta_stats.as_ref().map_or(vec![], |meta_stats| {
    meta_stats
      .meta_tag
      .iter()
      .map(|tag| (tag.key.to_string(), tag.value.to_string()))
      .collect()
  });
  let invalid_tag_chars: Regex = Regex::new(r#"[ ='"]"#).unwrap();
  for (key, value) in &meta_tags {
    if "source".eq(key.as_str()) {
      bail!("meta tag key cannot be \"source\"");
    }
    if invalid_tag_chars.is_match(key) || invalid_tag_chars.is_match(value) {
      bail!("meta tag keys and values cannot contain spaces, equals sign, or quotes");
    }
  }

  let meta_node_id = meta_stats.as_ref().and_then(|meta_stats| {
    meta_stats
      .node_id
      .as_ref()
      .and_then(env_or_inline_to_string)
  });

  Ok(StatsProvider::new(meta_node_id, meta_tags))
}

async fn update_config(
  pipeline: &MetricPipeline,
  new_config: Option<Arc<serde_yaml::Value>>,
) -> anyhow::Result<()> {
  let new_config = new_config.ok_or_else(|| anyhow!("configuration is not valid YAML"))?;
  let pipeline_config = yaml_value_to_proto(&new_config)?;
  pipeline.update_config(pipeline_config).await
}

pub async fn run_server<
  ShutdownFuture: Future<Output = ()>,
  K8sWatchFuture: Future<Output = anyhow::Result<Arc<PodsInfoSingleton>>> + Send + 'static,
>(
  config: Config,
  config_check_only: bool,
  shutdown: impl FnOnce() -> ShutdownFuture,
  shutdown_delay: Duration,
  hooks: impl ServerHooks,
  bind_resolver: Arc<dyn BindResolver>,
  singleton_manager: Arc<SingletonManager>,
  k8s_watch_factory: impl Fn() -> K8sWatchFuture + Send + Sync + 'static,
) -> anyhow::Result<()> {
  // Setup stats
  let stats_provider = build_stats_provider(&config)?;
  let scope = stats_provider.collector().scope(
    config
      .meta_stats
      .get_or_default()
      .meta_prefix
      .as_ref()
      .map_or("pulse_proxy", |c| c.as_str()),
  );
  scope.gauge("heartbeat").set(1);

  // Setup for pipeline load.
  let (initial_pipeline, fs_loader) = match config.pipeline_type.expect("pgv") {
    Pipeline_type::Pipeline(pipeline) => (pipeline, None),
    Pipeline_type::FsWatchedPipeline(fs_watched_pipeline) => {
      let stats = loader::Stats::new(&scope.scope("fs_config_loader"));
      let loader = WatchedFileLoader::new_loader(
        fs_watched_pipeline.dir,
        fs_watched_pipeline.file,
        |value: Option<serde_yaml::Value>| value.map(Arc::new),
        stats,
      )?;
      let config_yaml = loader
        .snapshot_watch()
        .borrow()
        .as_ref()
        .ok_or_else(|| anyhow!("configuration is not valid YAML"))?
        .clone();
      (yaml_value_to_proto(config_yaml.as_ref())?, Some(loader))
    },
  };

  let stats_shutdown_trigger = ComponentShutdownTrigger::default();

  let admin_state = AdminState::new(stats_provider.collector().clone());
  let pipeline = Arc::new(
    MetricPipeline::new_from_config(
      Arc::new(RealItemFactory {}),
      scope.scope("pipeline"),
      config.kubernetes.clone().unwrap_or_default(),
      Arc::new(move || k8s_watch_factory().boxed()),
      initial_pipeline,
      singleton_manager,
      admin_state.clone(),
      bind_resolver.clone(),
    )
    .await?,
  );

  if let Some(fs_loader) = &fs_loader {
    let mut snapshot_watch = fs_loader.snapshot_watch();
    let cloned_pipeline = pipeline.clone();
    let config_stats = ConfigStats::new(&scope);
    tokio::spawn(async move {
      while matches!(snapshot_watch.changed().await, Ok(())) {
        let new_config = snapshot_watch.borrow_and_update().as_ref().cloned();
        match update_config(&cloned_pipeline, new_config).await {
          Ok(()) => {
            log::info!("reloaded configuration from filesystem");
            config_stats.updated.inc();
          },
          Err(e) => {
            log::warn!("error reloading configuration from filesystem: {e}");
            config_stats.failed_update.inc();
          },
        }
      }
    });
  }

  if config_check_only {
    info!("--config-check-and-exit set, exiting");
    return Ok(());
  }

  let cloned_collector = stats_provider.collector().clone();
  if let Some(meta_stats) = config.meta_stats.as_ref() {
    let meta_stats_emitter = MetaStatsEmitter::new(
      stats_shutdown_trigger.make_shutdown(),
      stats_provider,
      meta_stats.meta_protocol.clone(),
      meta_stats.flush_interval.unwrap_duration_or(1.minutes()),
    )
    .await?;

    tokio::spawn(async move { meta_stats_emitter.run().await });
    info!("spawned meta stats server");
  }

  // Spawn admin server
  if let Some(admin) = config.admin.clone().into_option() {
    tokio::spawn(async move { admin_state.spawn_server(bind_resolver, &admin.bind).await });
  }

  info!("starting metric pipeline");
  pipeline.start().await;
  info!("metric pipeline started");
  hooks.server_started(cloned_collector).await;

  shutdown().await;

  if !shutdown_delay.is_zero() {
    info!(
      "waiting {:?} before shutting down (--shutdown-delay set)",
      shutdown_delay
    );
    tokio::time::sleep(shutdown_delay).await;
  }
  pipeline.shutdown().await;
  stats_shutdown_trigger.shutdown().await;
  if let Some(fs_loader) = fs_loader {
    fs_loader.shutdown().await;
  }
  info!("runtime terminated");
  Ok(())
}
