// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/licenses/strict/1.0.0.txt

use super::*;
use bd_shutdown::ComponentShutdownTrigger;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use time::Duration;

#[derive(Debug)]
struct MockClient {
  result: Mutex<anyhow::Result<()>>,
}

impl MockClient {
  fn new() -> Self {
    Self {
      result: Mutex::new(Ok(())),
    }
  }

  fn set_result(&self, result: anyhow::Result<()>) {
    let mut guard = self.result.lock().unwrap();
    // converting the error ref to a string and making a new error is a bit hacky but works for
    // mocks
    *guard = match result {
      Ok(()) => Ok(()),
      Err(e) => Err(anyhow::anyhow!(e.to_string())),
    };
  }
}

#[async_trait]
impl HealthCheckClient for MockClient {
  async fn check(&self) -> anyhow::Result<()> {
    match self.result.lock().unwrap().as_ref() {
      Ok(()) => Ok(()),
      Err(e) => anyhow::bail!(e.to_string()),
    }
  }
}

struct MockObserver {
  healthy_nodes: Mutex<Option<HashSet<String>>>,
  stats: Mutex<(u32, u32)>,
}

impl MockObserver {
  fn new() -> Self {
    Self {
      healthy_nodes: Mutex::new(None),
      stats: Mutex::new((0, 0)),
    }
  }
}

impl HealthCheckObserver for MockObserver {
  fn on_healthy_nodes_change(&self, healthy_nodes: &HashSet<String>) {
    *self.healthy_nodes.lock().unwrap() = Some(healthy_nodes.clone());
  }

  fn update_stats(&self, healthy_count: u32, total_count: u32) {
    *self.stats.lock().unwrap() = (healthy_count, total_count);
  }
}

#[test]
fn node_health_state_new() {
  let state = NodeHealthState::new();
  assert!(state.is_healthy);
  assert_eq!(state.consecutive_successes, 0);
  assert_eq!(state.consecutive_failures, 0);
}

#[test]
fn node_health_state_update_success() {
  let mut state = NodeHealthState::new();

  // Initially healthy
  assert!(!state.update("node1", &Ok(()), 3, 3));
  assert_eq!(state.consecutive_successes, 1);

  // Make unhealthy
  state.is_healthy = false;
  state.consecutive_successes = 0;

  // 1 success - not healthy yet
  assert!(!state.update("node1", &Ok(()), 3, 2));
  assert_eq!(state.consecutive_successes, 1);
  assert!(!state.is_healthy);

  // 2 success - becomes healthy
  assert!(state.update("node1", &Ok(()), 3, 2));
  assert_eq!(state.consecutive_successes, 2);
  assert!(state.is_healthy);
}

#[test]
fn node_health_state_update_failure() {
  let mut state = NodeHealthState::new();

  // 1 failure - still healthy
  assert!(!state.update("node1", &Err(anyhow::anyhow!("error")), 2, 3));
  assert_eq!(state.consecutive_failures, 1);
  assert!(state.is_healthy);

  // 2 failures - becomes unhealthy
  assert!(state.update("node1", &Err(anyhow::anyhow!("error")), 2, 3));
  assert_eq!(state.consecutive_failures, 2);
  assert!(!state.is_healthy);

  // Subsequent failure - stays unhealthy
  assert!(!state.update("node1", &Err(anyhow::anyhow!("error")), 2, 3));
  assert_eq!(state.consecutive_failures, 3);
  assert!(!state.is_healthy);
}

#[tokio::test(start_paused = true)]
async fn health_check_loop_basic_flow() {
  let config = HealthCheckConfig {
    interval: Duration::milliseconds(10),
    failure_threshold: 2,
    success_threshold: 2,
  };

  let client1 = Arc::new(MockClient::new());
  let client2 = Arc::new(MockClient::new());

  let mut nodes = HashMap::new();
  nodes.insert(
    "node1".to_string(),
    client1.clone() as Arc<dyn HealthCheckClient>,
  );
  nodes.insert(
    "node2".to_string(),
    client2.clone() as Arc<dyn HealthCheckClient>,
  );

  let observer = Arc::new(MockObserver::new());
  let observer_clone = observer.clone();

  let shutdown_trigger = ComponentShutdownTrigger::default();
  let shutdown = shutdown_trigger.make_shutdown();

  // Run the loop in background
  tokio::spawn(async move {
    run_health_check_loop(config, nodes, observer_clone, shutdown).await;
  });

  // Allow the spawned task to initialize and hit the first sleep
  tokio::task::yield_now().await;

  // Allow initial cycle to run
  tokio::time::advance(std::time::Duration::from_millis(50)).await;
  tokio::task::yield_now().await;

  {
    let stats = observer.stats.lock().unwrap();
    assert_eq!(stats.0, 2); // all healthy
    assert_eq!(stats.1, 2);
  }

  // Set node1 to fail
  client1.set_result(Err(anyhow::anyhow!("error")));

  // Wait for failure threshold.
  // The failure threshold is 2, so we need at least 2 full cycles of the health check loop.
  // We advance time in small steps (15ms > 10ms interval) and yield after each step.
  // This is crucial because `advance` might fire the current timer, but the background task
  // needs to execute and `await` the *next* sleep for the next timer to be registered.
  // Iterating 5 times provides a generous safety buffer ensuring > 2 cycles execute.
  for _ in 0 .. 5 {
    tokio::time::advance(std::time::Duration::from_millis(15)).await;
    tokio::task::yield_now().await;
  }

  {
    let stats = observer.stats.lock().unwrap();
    assert_eq!(stats.0, 1);
    assert_eq!(stats.1, 2);

    let healthy = observer.healthy_nodes.lock().unwrap();
    let healthy_set = healthy.as_ref().unwrap();
    assert!(healthy_set.contains("node2"));
    assert!(!healthy_set.contains("node1"));
  }

  // Set node1 to succeed
  client1.set_result(Ok(()));

  // Wait for success threshold.
  // Similar to above, we allow multiple cycles to run to cross the success threshold (2).
  for _ in 0 .. 5 {
    tokio::time::advance(std::time::Duration::from_millis(15)).await;
    tokio::task::yield_now().await;
  }

  {
    let stats = observer.stats.lock().unwrap();
    assert_eq!(stats.0, 2);
    assert_eq!(stats.1, 2);

    let healthy = observer.healthy_nodes.lock().unwrap();
    let healthy_set = healthy.as_ref().unwrap();
    assert!(healthy_set.contains("node1"));
    assert!(healthy_set.contains("node2"));
  }
}
