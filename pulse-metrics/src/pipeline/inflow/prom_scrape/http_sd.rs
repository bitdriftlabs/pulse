// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::scraper::Ticker;
use bd_shutdown::ComponentShutdown;
use k8s_prom::kubernetes_prometheus_config::HttpServiceDiscovery;
use pulse_common::proto::env_or_inline_to_string;
use pulse_protobuf::protos::pulse::config::inflow::v1::k8s_prom;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use time::ext::NumericalStdDuration;
use tokio::sync::watch;
use anyhow::anyhow;

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct TargetBlock {
  pub targets: Vec<String>,
  pub labels: BTreeMap<String, String>,
}

async fn do_fetch(config: &HttpServiceDiscovery) -> anyhow::Result<Vec<TargetBlock>> {
  log::debug!(
    "Fetching HTTP service discovery targets from {}",
    env_or_inline_to_string(&config.url).unwrap_or_default()
  );
  let response = reqwest::Client::new()
    .get(env_or_inline_to_string(&config.url).ok_or_else(|| anyhow!("HTTP SD URL is not set"))?)
    .timeout(15.std_seconds())
    .send()
    .await?;
  let target_blocks: Vec<TargetBlock> = response.json().await?;
  log::debug!("Fetched target blocks: {:?}", target_blocks);
  Ok(target_blocks)
}

async fn do_fetch_and_send(config: &HttpServiceDiscovery, tx: &watch::Sender<Vec<TargetBlock>>) {
  match do_fetch(config).await {
    Ok(targets) => {
      tx.send_if_modified(|existing_targets| {
        if targets == *existing_targets {
          log::debug!("HTTP service discovery targets unchanged, not sending update");
          false
        } else {
          *existing_targets = targets;
          true
        }
      });
    },
    Err(e) => {
      log::warn!("Failed to fetch HTTP service discovery targets: {e}");
    },
  }
}

pub async fn make_fetcher(
  config: HttpServiceDiscovery,
  mut shutdown: ComponentShutdown,
  mut ticker: Box<dyn Ticker>,
) -> watch::Receiver<Vec<TargetBlock>> {
  let initial_targets = do_fetch(&config)
    .await
    .inspect_err(|e| log::warn!("Failed to fetch initial HTTP service discovery targets: {e}"))
    .unwrap_or_default();
  let (tx, rx) = watch::channel(initial_targets);
  tokio::spawn(async move {
    loop {
      tokio::select! {
        () = ticker.next() => do_fetch_and_send(&config, &tx).await,
        () = shutdown.cancelled() => {
          log::debug!("HTTP service discovery fetcher received shutdown signal");
          break;
        }
      }
    }
    log::debug!("HTTP service discovery fetcher shutting down");
  });

  rx
}
