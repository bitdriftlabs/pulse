// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./batch_test.rs"]
mod batch_test;

use bd_server_stats::stats::Scope;
use bd_shutdown::ComponentShutdown;
use bd_time::TimeDurationExt;
use parking_lot::Mutex;
use prometheus::{IntCounter, IntGauge};
use pulse_common::proto::ProtoDurationToStdDuration;
use pulse_protobuf::protos::pulse::config::outflow::v1::queue_policy::QueuePolicy;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::Arc;
use time::Duration;
use tokio::sync::Notify;
use tokio::task::JoinHandle;

const DEFAULT_BATCH_FILL_WAIT: Duration = Duration::milliseconds(50);
const DEFAULT_QUEUE_MAX_BYTES: u64 = 8 * 1024 * 1024;

// A Batch is a collection of items that are collected over a period of time.
pub trait Batch<I> {
  // Push an item onto the batch. If the batch is complete, return the size.
  fn push(&mut self, item: I) -> Option<usize>;

  // Finish the batch. Will only be called if there is a batch fill timeout. Return the size.
  fn finish(&mut self) -> usize;
}

// A combined batch and its size. Used for construction of both pending batches as well as entries
// in the LIFO queue.
struct QueueEntry<T> {
  size: usize,
  item: T,
}

// Data in the batch builder that must be locked.
struct LockedData<T> {
  batch_queue: VecDeque<QueueEntry<T>>,
  pending_batch: Option<T>,
  current_total_size: usize,
  batch_fill_wait_task: Option<JoinHandle<()>>,
  shutdown: bool,
  waiters: bool,
}

// Stats for the batch builder.
struct Stats {
  dropped_bytes: IntCounter,
  queued_bytes: IntGauge,
}

//
// BatchBuilder
//

// The batch builder combines the ability to create generic batches of items, as well as a LIFO
// queue of completed batches. The total size of both completed batches and pending batches are
// tracked, and if there is overflow, the oldest entries are evicted from the LIFO queue to make
// room.
// The LIFO requirement means we cannot use tokio::mpsc, which is why this file contains so much
// manual synchronization code.
// TODO(mattklein123): Potentially make the LIFO configurable. Some backends might not like this?
// (Though if we send in parallel seems like it shouldn't matter.)
pub struct BatchBuilder<I, B: Batch<I>> {
  locked_data: Mutex<LockedData<B>>,
  notify_on_data: Notify,
  constructor: Box<dyn Fn() -> B + Send + Sync>,
  batch_fill_wait_duration: Duration,
  max_total_bytes: usize,
  stats: Stats,
  phantom: PhantomData<I>,
}

impl<I: Send + Sync + 'static, B: Batch<I> + Send + Sync + 'static> BatchBuilder<I, B> {
  // Create a new batch builder with a given policy, batch constructor, and shutdown notifier.
  pub fn new(
    scope: &Scope,
    policy: &QueuePolicy,
    constructor: impl Fn() -> B + Send + Sync + 'static,
    mut shutdown: ComponentShutdown,
  ) -> Arc<Self> {
    let scope = scope.scope("batch");
    let stats = Stats {
      dropped_bytes: scope.counter("dropped_bytes"),
      queued_bytes: scope.gauge("queued_bytes"),
    };

    let batch_builder = Arc::new(Self {
      locked_data: Mutex::new(LockedData {
        batch_queue: VecDeque::new(),
        pending_batch: None,
        current_total_size: 0,
        batch_fill_wait_task: None,
        shutdown: false,
        waiters: false,
      }),
      notify_on_data: Notify::new(),
      constructor: Box::new(constructor),
      batch_fill_wait_duration: policy
        .batch_fill_wait
        .unwrap_duration_or(DEFAULT_BATCH_FILL_WAIT),
      max_total_bytes: policy
        .queue_max_bytes
        .unwrap_or(DEFAULT_QUEUE_MAX_BYTES)
        .try_into()
        .unwrap(),
      stats,
      phantom: PhantomData,
    });

    // Spawn a task that will wait for shutdown.
    let cloned_batch_builder = batch_builder.clone();
    tokio::spawn(async move {
      shutdown.cancelled().await;
      log::trace!("shutting down");
      let mut locked_data = cloned_batch_builder.locked_data.lock();
      locked_data.shutdown = true;

      // Complete any pending batch.
      if locked_data.pending_batch.is_some() {
        Self::finish_batch(
          &mut locked_data,
          &cloned_batch_builder.stats,
          &cloned_batch_builder.notify_on_data,
          None,
        );
      }

      // Notify all waiters so they can all exit after consuming any remaining items.
      cloned_batch_builder.notify_on_data.notify_waiters();
      drop(shutdown);
    });

    batch_builder
  }

  // Increment the total size across internal state and stats.
  fn inc_total_size(locked_data: &mut LockedData<B>, stats: &Stats, size: usize) {
    locked_data.current_total_size += size;
    stats.queued_bytes.add(size.try_into().unwrap());
  }

  // Decrement the total size across internal state and stats.
  fn dec_total_size(locked_data: &mut LockedData<B>, stats: &Stats, size: usize) {
    debug_assert!(locked_data.current_total_size >= size);
    locked_data.current_total_size -= size;
    stats.queued_bytes.sub(size.try_into().unwrap());
  }

  // Abort the fill task if it exists.
  fn maybe_abort_fill_task(locked_data: &mut LockedData<B>) {
    if let Some(handle) = locked_data.batch_fill_wait_task.take() {
      log::trace!("aborting batch fill wait task");
      handle.abort();
    }
  }

  // Send an item through the batch builder. This may result in old data getting dropped if the
  // total data in the LIFO queue and the pending batch is too large.
  // TODO(mattklein123): Consider sending multiple items through in a single call to avoid lock and
  // wake thrash.
  pub fn send(self: &Arc<Self>, item: I) {
    let mut locked_data = self.locked_data.lock();
    if locked_data.shutdown {
      // Just silently drop the data.
      // TODO(mattklein123): At least keep track of this in stats.
      return;
    }

    let pending_batch = locked_data.pending_batch.get_or_insert_with(|| {
      log::trace!("creating new pending batch");
      (self.constructor)()
    });

    // If the batch is finished, process it.
    if let Some(finished_size) = pending_batch.push(item) {
      Self::maybe_abort_fill_task(&mut locked_data);

      if finished_size > self.max_total_bytes {
        // If somehow we just pushed an item that makes the pending batch bigger than the size of
        // the queue, this will break all of the math below so we have to drop it. For now
        // just drop the entire batch since there is no good way to unpush it right now and
        // this is an extreme edge case so we should just not crash. We can revisit later if
        // an issue.
        self
          .stats
          .dropped_bytes
          .inc_by(finished_size.try_into().unwrap());
        locked_data.pending_batch.take();
        return;
      }

      // See if we need to evict old entries to make room for new data.
      while self.max_total_bytes - locked_data.current_total_size < finished_size {
        let back = locked_data.batch_queue.pop_back().unwrap();
        log::trace!("evicting with size: {}", back.size);
        self
          .stats
          .dropped_bytes
          .inc_by(back.size.try_into().unwrap());
        Self::dec_total_size(&mut locked_data, &self.stats, back.size);
      }

      Self::finish_batch(
        &mut locked_data,
        &self.stats,
        &self.notify_on_data,
        Some(finished_size),
      );
    } else if locked_data.batch_fill_wait_task.is_none() {
      // If there is no fill task, start one.
      let cloned_self = self.clone();
      log::trace!("spawning batch wait task");
      locked_data.batch_fill_wait_task = Some(tokio::spawn(async move {
        cloned_self.batch_fill_wait_duration.sleep().await;
        log::trace!("batch fill timeout elapsed");
        let mut locked_data = cloned_self.locked_data.lock();
        locked_data.batch_fill_wait_task.take();
        Self::finish_batch(
          &mut locked_data,
          &cloned_self.stats,
          &cloned_self.notify_on_data,
          None,
        );
      }));
    }
  }

  // Finish a pending batch and push it onto the LIFO queue.
  fn finish_batch(
    locked_data: &mut LockedData<B>,
    stats: &Stats,
    notify_on_data: &Notify,
    size: Option<usize>,
  ) {
    // It's possible for this to race with batch max so make sure we still have a pending batch.
    // TODO(mattklein123): Should we handle this differently? Perhaps it would be better to not
    // spawn a task every time we need a fill timeout? We could have a task that just sits around
    // and waits to be asked to timeout, and then we cancel the timeout? Alternatively we could
    // keep track of a "batch epoch" to make sure we don't finish the wrong batch?
    let Some(mut pending_batch) = locked_data.pending_batch.take() else {
      return;
    };
    let size = size.unwrap_or_else(|| pending_batch.finish());
    Self::inc_total_size(locked_data, stats, size);

    // In order to avoid spurious wakeups, we keep track of whether there are any waiters.
    // TODO(mattklein123): Due to the use of multiple batch builders in the Lyft specific config
    // case of Prom remote write, we now have cancellation inside this code. Thus, we will still
    // end up calling notify_waiters() when there are no waiters, though it won't store any permits
    // so won't lead to spurious wakeups when the waits start again. With that said, given that
    // we only ever have 1 consumer for this code, we could likely improve all of this by not
    // using Notify and a custom future/waker implementation if we care to in the future.
    if locked_data.waiters {
      locked_data.waiters = false;
      notify_on_data.notify_waiters();
    }
    log::trace!("finalizing and pushing batch with size: {}", size);
    locked_data.batch_queue.push_front(QueueEntry {
      size,
      item: pending_batch,
    });
  }

  // Get the next set of batches. Returns None when the builder has been shutdown and
  // there is no more data.
  pub async fn next_batch_set(&self, max_items: Option<usize>) -> Option<Vec<B>> {
    loop {
      let notified_future = {
        let mut locked_data = self.locked_data.lock();
        let len_to_pop = locked_data
          .batch_queue
          .len()
          .min(max_items.unwrap_or(locked_data.batch_queue.len()));
        if len_to_pop > 0 {
          let mut batch_set = Vec::with_capacity(len_to_pop);
          for _ in 0 .. len_to_pop {
            let entry = locked_data.batch_queue.pop_front().unwrap();
            Self::dec_total_size(&mut locked_data, &self.stats, entry.size);
            log::trace!("popping entry with size: {}", entry.size);
            batch_set.push(entry.item);
          }
          return Some(batch_set);
        }
        if locked_data.shutdown {
          return None;
        }

        // Make sure we don't miss a notification before dropping the lock. enable() is called which
        // will consume a notify_waiters() call, prior to dropping the lock.
        log::trace!("queue is empty, waiting for item");
        let mut notified_future = Box::pin(self.notify_on_data.notified());
        notified_future.as_mut().enable();
        locked_data.waiters = true;
        notified_future
      };

      notified_future.await;
    }
  }
}
