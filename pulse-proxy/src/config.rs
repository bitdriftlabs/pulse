// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./config_test.rs"]
mod config_test;

use anyhow::anyhow;
use bd_runtime_config::loader::{self, ConfigPtr, Loader, WatchedFileLoader};
use bd_server_stats::stats::Scope;
use bd_shutdown::ComponentShutdown;
use futures::future::select_all;
use futures::FutureExt;
use itertools::Itertools;
use protobuf::MessageFull;
use pulse_common::proto::{yaml_to_proto, yaml_value_to_proto};
use pulse_protobuf::protos::pulse::config::bootstrap::v1::bootstrap::config::Pipeline_type;
use pulse_protobuf::protos::pulse::config::bootstrap::v1::bootstrap::PipelineConfig;
use pulse_protobuf::protos::pulse::config::common::v1::common::RuntimeConfig;
use serde_yaml::Value;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};

pub fn load_from_file<T: MessageFull>(path: &str) -> anyhow::Result<T> {
  let file_contents = std::fs::read_to_string(path)?;
  yaml_to_proto(&file_contents)
}

//
// ConfigLoader
//

pub struct ConfigLoader {
  bootstrap: PipelineConfig,
  layers: Vec<Arc<dyn Loader<Value>>>,
  watches: Vec<watch::Receiver<ConfigPtr<Value>>>,
  tx: mpsc::Sender<anyhow::Result<PipelineConfig>>,
}

impl ConfigLoader {
  pub async fn initialize(
    config: Pipeline_type,
    scope: &Scope,
    shutdown: ComponentShutdown,
  ) -> anyhow::Result<mpsc::Receiver<anyhow::Result<PipelineConfig>>> {
    fn make_loader(
      fs_watched_pipeline: RuntimeConfig,
      scope: &Scope,
    ) -> anyhow::Result<Arc<dyn Loader<Value>>> {
      let stats = loader::Stats::new(&scope.scope("fs_config_loader"));
      Ok(WatchedFileLoader::new_loader(
        fs_watched_pipeline.dir,
        fs_watched_pipeline.file,
        |value: Option<Value>| value.map(Arc::new),
        stats,
      )?)
    }

    let (bootstrap, layers) = match config {
      Pipeline_type::Pipeline(pipeline) => (pipeline, vec![]),
      Pipeline_type::FsWatchedPipeline(fs_watched_pipeline) => (
        PipelineConfig::default(),
        vec![make_loader(fs_watched_pipeline, scope)?],
      ),
      Pipeline_type::MergedPipeline(merged_pipeline) => {
        let loaders = merged_pipeline
          .fs_watched_pipelines
          .into_iter()
          .map(|fs_watched_pipeline| make_loader(fs_watched_pipeline, scope))
          .try_collect()?;
        (merged_pipeline.bootstrap.unwrap_or_default(), loaders)
      },
    };

    let (tx, rx) = mpsc::channel(1);
    let watches = layers.iter().map(|l| l.snapshot_watch()).collect();
    let mut config_loader = Self {
      bootstrap,
      layers,
      watches,
      tx,
    };
    let initial_config = config_loader.reload()?;
    config_loader.tx.send(Ok(initial_config)).await?;

    if !config_loader.layers.is_empty() {
      tokio::spawn(async move {
        config_loader.reload_loop(shutdown).await;
      });
    }

    Ok(rx)
  }

  async fn reload_loop(&mut self, mut shutdown: ComponentShutdown) {
    loop {
      let watch_futures = self.watches.iter_mut().map(|w| w.changed().boxed());

      tokio::select! {
        () = shutdown.cancelled() => break,
        _ = select_all(watch_futures) => {},
      }

      let new_config = self.reload();
      let _ignored = self.tx.send(new_config).await;
    }

    for loader in &self.layers {
      loader.shutdown().await;
    }
  }

  fn reload(&mut self) -> anyhow::Result<PipelineConfig> {
    let mut new_config = self.bootstrap.clone();
    for watch in &mut self.watches {
      let config_yaml = watch
        .borrow_and_update()
        .as_ref()
        .ok_or_else(|| anyhow!("configuration is not valid YAML"))?
        .clone();
      let layer: PipelineConfig = yaml_value_to_proto(config_yaml.as_ref())?;
      for (name, inflow) in layer.inflows {
        new_config.inflows.insert(name, inflow);
      }
      for (name, processor) in layer.processors {
        new_config.processors.insert(name, processor);
      }
      for (name, outflow) in layer.outflows {
        new_config.outflows.insert(name, outflow);
      }

      // Currently if advanced config is present, it will overwrite the previous value.
      if let Some(advanced) = layer.advanced.into_option() {
        new_config.advanced = Some(advanced).into();
      }
    }

    Ok(new_config)
  }
}
