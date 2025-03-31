// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

// TODO(mattklein123): This is an async version of the variant in loop-sdk that is used to test the
// ring buffer. We should move both of these to a common location / loop-api.

#[derive(Default)]
struct EntryLockedData {
  wait_on: bool,
  at_barrier: bool,
  signaled: bool,
}

#[derive(Default)]
struct Entry {
  at_barrier_condition: Notify,
  signal_condition: Notify,
  locked_data: Mutex<EntryLockedData>,
}

#[derive(Default)]
pub struct ThreadSynchronizer {
  data: Mutex<HashMap<String, Arc<Entry>>>,
}

impl ThreadSynchronizer {
  async fn entry(&self, name: &str) -> Arc<Entry> {
    self
      .data
      .lock()
      .await
      .entry(name.to_string())
      .or_default()
      .clone()
  }

  // This is the only API that should generally be called from production code. It introduces a
  // "sync point" that test code can then use to force blocking, thread barriers, etc. The
  // sync_point() will do nothing unless it has been registered to block via wait_on().
  pub async fn sync_point(&self, name: &str) {
    let entry = self.entry(name).await;
    let mut locked_data = entry.locked_data.lock().await;

    // See if we are ignoring waits. If so, just return.
    if !locked_data.wait_on {
      log::debug!("sync point {name}: ignoring");
      return;
    }
    locked_data.wait_on = false;

    // See if we are already signaled. If so, just clear signaled and return.
    if locked_data.signaled {
      log::debug!("sync point {name}: already signaled");
      locked_data.signaled = false;
      return;
    }

    // Now signal any barrier waiters.
    locked_data.at_barrier = true;
    entry.at_barrier_condition.notify_one();

    // Now wait to be signaled.
    log::debug!("blocking on sync point {name}");
    while !locked_data.signaled {
      let mut notified = Box::pin(entry.signal_condition.notified());
      notified.as_mut().enable();
      drop(locked_data);
      notified.await;
      locked_data = entry.locked_data.lock().await;
    }
    log::debug!("done blocking for sync point {name}");

    // Clear the barrier and signaled before unlocking and returning.
    assert!(locked_data.at_barrier);
    locked_data.at_barrier = false;
    assert!(locked_data.signaled);
    locked_data.signaled = false;
  }

  // The next time the sync point registered with name is invoked via sync_point(), the calling code
  // will block until signaled. Note that this is a one-shot operation and the sync point's wait
  // status will be cleared.
  pub async fn wait_on(&self, name: &str) {
    let entry = self.entry(name).await;
    let mut locked_data = entry.locked_data.lock().await;
    log::debug!("waiting on next {name}");
    assert!(!locked_data.wait_on);
    locked_data.wait_on = true;
  }

  // This call will block until the next time the sync point registered with name is invoked. The
  // name must have been previously registered for blocking via wait_on(). The typical test pattern
  // is to have a thread arrive at a sync point, block, and then release a test thread which
  // continues test execution, eventually calling signal() to release the other thread.
  pub async fn barrier_on(&self, name: &str) {
    let entry = self.entry(name).await;
    let mut locked_data = entry.locked_data.lock().await;
    log::debug!("barrier on {name}");
    while !locked_data.at_barrier {
      let mut notified = Box::pin(entry.at_barrier_condition.notified());
      notified.as_mut().enable();
      drop(locked_data);
      notified.await;
      locked_data = entry.locked_data.lock().await;
    }
    log::debug!("barrier complete {name}");
  }

  // Signal an event such that a thread that is blocked within sync_point() will now proceed.
  pub async fn signal(&self, name: &str) {
    let entry = self.entry(name).await;
    let mut locked_data = entry.locked_data.lock().await;
    assert!(!locked_data.signaled);
    log::debug!("signaling {name}");
    locked_data.signaled = true;
    entry.signal_condition.notify_one();
  }
}
