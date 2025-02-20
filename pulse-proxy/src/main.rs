// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use anyhow::Context;
use bd_server_stats::stats::Collector;
use clap::Parser;
use log::info;
use pulse_common::bind_resolver::RealBindResolver;
use pulse_common::global_initialize;
use pulse_common::k8s::pods_info::PodsInfoSingleton;
use pulse_common::singleton::SingletonManager;
use pulse_protobuf::protos::pulse::config::bootstrap::v1::bootstrap::Config;
use pulse_proxy::{run_server, ServerHooks};
use std::num::NonZeroUsize;
use std::sync::Arc;
use tikv_jemallocator::Jemalloc;
use tokio::select;
use tokio::signal::unix::{signal, SignalKind};
use tokio::time::Duration;

#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[allow(clippy::needless_raw_string_hashes)]
pub mod built_info {
  include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

#[derive(Parser, Debug, Clone)]
struct Options {
  #[arg(short = 'c', long = "config")]
  pub config: String,

  #[arg(long = "config-check-and-exit")]
  pub config_check: bool,

  #[arg(long = "version")]
  pub version: bool,

  #[arg(long = "shutdown-delay", default_value = "0")]
  pub shutdown_delay: u32,
}

struct NullHooks {}

#[async_trait::async_trait]
impl ServerHooks for NullHooks {
  async fn server_started(&self, _collector: Collector) {}
}

fn main() -> anyhow::Result<()> {
  global_initialize();
  let opts = Options::parse();

  if opts.version {
    println!(
      "pulse-proxy: {}",
      built_info::GIT_COMMIT_HASH.unwrap_or("unknown")
    );
    return Ok(());
  }
  info!(
    "pulse-proxy loading: {}",
    built_info::GIT_COMMIT_HASH.unwrap_or("unknown")
  );

  let config: Config = pulse_proxy::config::load_from_file(opts.config.as_ref())
    .with_context(|| format!("can't load config file from {}", opts.config))?;
  info!("loaded config file {}", opts.config);

  let num_threads = std::thread::available_parallelism().unwrap_or_else(|_| {
    log::warn!("could not determine number of CPUs. Defaulting to 1");
    NonZeroUsize::new(1).unwrap()
  });
  log::info!("running server with {num_threads} workers");
  let runtime = tokio::runtime::Builder::new_multi_thread()
    .worker_threads(num_threads.into())
    .enable_all()
    .build()
    .unwrap();

  let singleton_manager = Arc::new(SingletonManager::default());
  let cloned_singleton_manager = singleton_manager.clone();
  let k8s_config = config.kubernetes.get_or_default().clone();
  runtime.block_on(async {
    run_server(
      config,
      opts.config_check,
      || async {
        // Trap ctrl+c and sigterm messages and perform a clean shutdown
        let mut sigint = signal(SignalKind::interrupt()).unwrap();
        let mut sigterm = signal(SignalKind::terminate()).unwrap();
        select! {
          _ = sigint.recv() => info!("received sigint"),
          _ = sigterm.recv() => info!("received sigterm"),
        }
      },
      Duration::from_secs(opts.shutdown_delay.into()),
      NullHooks {},
      Arc::new(RealBindResolver {}),
      singleton_manager,
      move || PodsInfoSingleton::get(cloned_singleton_manager.clone(), k8s_config.clone()),
    )
    .await
  })
}
