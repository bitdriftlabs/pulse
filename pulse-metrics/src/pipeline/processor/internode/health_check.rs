// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./health_check_test.rs"]
mod health_check_test;

use async_trait::async_trait;
use bd_shutdown::ComponentShutdown;
use futures::StreamExt;
use log::{info, warn};
use std::collections::{HashMap, HashSet};
use time::Duration;

/// Configuration for the health check loop.
#[derive(Debug, Clone)]
pub struct HealthCheckConfig {
  pub interval: Duration,
  pub failure_threshold: u32,
  pub success_threshold: u32,
}

/// Helper struct to track the health state of a single node.
#[derive(Debug)]
struct NodeHealthState {
  consecutive_successes: u32,
  consecutive_failures: u32,
  is_healthy: bool,
}

impl NodeHealthState {
  fn new() -> Self {
    Self {
      consecutive_successes: 0,
      consecutive_failures: 0,
      is_healthy: true, // Assume healthy to start
    }
  }

  /// Updates the state based on a health check result. Returns true if the health status changed.
  #[must_use]
  fn update(
    &mut self,
    node_id: &str,
    result: &anyhow::Result<()>,
    failure_threshold: u32,
    success_threshold: u32,
  ) -> bool {
    match result {
      Ok(()) => {
        self.consecutive_successes += 1;
        self.consecutive_failures = 0;
        if !self.is_healthy && self.consecutive_successes >= success_threshold {
          self.is_healthy = true;
          info!("node {node_id} marked healthy");
          return true;
        }
      },
      Err(e) => {
        self.consecutive_successes = 0;
        self.consecutive_failures += 1;
        if self.is_healthy && self.consecutive_failures >= failure_threshold {
          self.is_healthy = false;
          warn!("node {node_id} marked unhealthy: {e}");
          return true;
        }
      },
    }
    false
  }
}

/// Trait for checking a node's health.
#[async_trait]
pub trait HealthCheckClient: Send + Sync + 'static {
  /// Checks the health of the node.
  async fn check(&self) -> anyhow::Result<()>;
}

/// Trait to handle updates to the set of healthy nodes and stats.
pub trait HealthCheckObserver: Send + Sync + 'static {
  /// Called when the set of healthy nodes changes.
  fn on_healthy_nodes_change(&self, healthy_nodes: &HashSet<String>);

  /// Called periodically to update metrics.
  fn update_stats(&self, healthy_count: u32, total_count: u32);
}

/// Runs the health check loop.
pub async fn run_health_check_loop(
  config: HealthCheckConfig,
  clients: HashMap<String, std::sync::Arc<dyn HealthCheckClient>>,
  observer: std::sync::Arc<dyn HealthCheckObserver>,
  mut shutdown: ComponentShutdown,
) {
  info!(
    "starting internode health check loop with interval: {:?}, failure_threshold: {}, \
     success_threshold: {}",
    config.interval, config.failure_threshold, config.success_threshold
  );

  let mut node_states: HashMap<String, NodeHealthState> = HashMap::new();
  for node_id in clients.keys() {
    node_states.insert(node_id.clone(), NodeHealthState::new());
  }

  // Notify observer of initial state (all nodes assumed healthy).
  let initial_healthy: HashSet<String> = node_states.keys().cloned().collect();
  observer.update_stats(
    initial_healthy.len().try_into().unwrap(),
    clients.len().try_into().unwrap(),
  );
  observer.on_healthy_nodes_change(&initial_healthy);

  let sleep_duration: std::time::Duration = config.interval.try_into().expect("invalid interval");

  loop {
    // Perform health checks immediately, then sleep.
    let mut requests = futures::stream::FuturesUnordered::new();
    for (node_id, client) in &clients {
      let client = client.clone();
      let node_id = node_id.clone();
      requests.push(async move {
        let result = client.check().await;
        (node_id, result)
      });
    }

    let mut changed = false;
    while let Some((node_id, result)) = requests.next().await {
      if let Some(state) = node_states.get_mut(&node_id) {
        changed = state.update(
          &node_id,
          &result,
          config.failure_threshold,
          config.success_threshold,
        ) || changed;
      }
    }

    let healthy_nodes: HashSet<String> = node_states
      .iter()
      .filter(|(_, state)| state.is_healthy)
      .map(|(id, _)| id.clone())
      .collect();

    observer.update_stats(
      healthy_nodes.len().try_into().unwrap(),
      clients.len().try_into().unwrap(),
    );

    if changed {
      observer.on_healthy_nodes_change(&healthy_nodes);
    }

    tokio::select! {
      () = shutdown.cancelled() => {
        info!("health check loop shutting down");
        return;
      }
      () = tokio::time::sleep(sleep_duration) => {}
    }
  }
}
