// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::*;
use bd_server_stats::stats::Collector;
use bd_shutdown::ComponentShutdownTrigger;
use futures::poll;
use std::iter::once;
use std::task::Poll;
use time::ext::NumericalDuration;
use tokio::pin;

#[derive(Debug, PartialEq, Copy, Clone)]
enum TestBatchResult {
  Complete(usize),
  ExpectFinish(usize),
}

const fn expect_finish(size: usize) -> Option<TestBatchResult> {
  Some(TestBatchResult::ExpectFinish(size))
}

const fn complete(size: usize) -> Option<TestBatchResult> {
  Some(TestBatchResult::Complete(size))
}

fn map_return(batches: Option<Vec<TestBatch>>) -> Option<Vec<Vec<&'static str>>> {
  batches.map(|b| b.into_iter().map(|b| b.items).collect())
}

#[derive(Debug, Default, PartialEq)]
struct TestBatch {
  items: Vec<&'static str>,
  result: Option<TestBatchResult>,
}

impl Batch<(&'static str, Option<TestBatchResult>)> for TestBatch {
  fn push(
    &mut self,
    items: impl Iterator<Item = (&'static str, Option<TestBatchResult>)>,
  ) -> Option<usize> {
    for item in items {
      self.items.push(item.0);
      self.result = item.1;
      if let Some(TestBatchResult::Complete(size)) = item.1 {
        return Some(size);
      }
    }
    None
  }

  fn finish(&mut self) -> usize {
    if let Some(TestBatchResult::ExpectFinish(size)) = self.result {
      return size;
    }
    unreachable!();
  }
}

#[tokio::test(start_paused = true)]
async fn many_items_batch() {
  let shutdown_trigger = ComponentShutdownTrigger::default();
  let batch_builder = BatchBuilder::new(
    &Collector::default().scope("test"),
    &QueuePolicy::default(),
    TestBatch::default,
    shutdown_trigger.make_shutdown(),
  );
  batch_builder.send(
    [
      ("a", None),
      ("b", complete(2)),
      ("c", None),
      ("d", complete(2)),
    ]
    .into_iter(),
  );
  assert_eq!(
    vec![vec!["c", "d"], vec!["a", "b"]],
    map_return(batch_builder.next_batch_set(None).await).unwrap()
  );
}

#[tokio::test(start_paused = true)]
async fn multiple_queues() {
  let shutdown_trigger = ComponentShutdownTrigger::default();
  let batch_builder = BatchBuilder::new(
    &Collector::default().scope("test"),
    &QueuePolicy {
      concurrent_batch_queues: Some(2),
      ..Default::default()
    },
    TestBatch::default,
    shutdown_trigger.make_shutdown(),
  );
  batch_builder.send(
    [
      ("a", None),
      ("b", complete(2)),
      ("c", None),
      ("d", complete(2)),
    ]
    .into_iter(),
  );
  assert_eq!(
    vec![vec!["c", "d"], vec!["a", "b"]],
    map_return(batch_builder.next_batch_set(None).await).unwrap()
  );
}

#[tokio::test(start_paused = true)]
async fn simple_batch() {
  let shutdown_trigger = ComponentShutdownTrigger::default();
  let batch_builder = BatchBuilder::new(
    &Collector::default().scope("test"),
    &QueuePolicy::default(),
    TestBatch::default,
    shutdown_trigger.make_shutdown(),
  );

  let batch_future = batch_builder.next_batch_set(None);
  pin!(batch_future);
  assert_eq!(Poll::Pending, poll!(batch_future.as_mut()));

  batch_builder.send(once(("a", expect_finish(4096))));
  assert_eq!(Poll::Pending, poll!(batch_future.as_mut()));

  // Sleep to advance past the batch fill time.
  51.milliseconds().sleep().await;
  assert_eq!(Some(vec![vec!["a"]]), map_return(batch_future.await));

  // Add 2 items that together will meet the max batch size. This should flush it and cancel the
  // fill wait task.
  batch_builder.send(once(("a", None)));
  batch_builder.send(once(("b", complete(4096))));
  51.milliseconds().sleep().await;
  assert_eq!(
    Some(vec![vec!["a", "b"]]),
    map_return(batch_builder.next_batch_set(Some(1)).await)
  );

  // Add another item, and advance past the batch fill time and make sure we get it back.
  batch_builder.send(once(("a", expect_finish(1))));
  51.milliseconds().sleep().await;
  assert_eq!(
    Some(vec![vec!["a"]]),
    map_return(batch_builder.next_batch_set(Some(16)).await)
  );

  // Add another item, and advance past the batch fill time and make sure we get it back.
  batch_builder.send(once(("b", expect_finish(1))));
  51.milliseconds().sleep().await;
  assert_eq!(
    Some(vec![vec!["b"]]),
    map_return(batch_builder.next_batch_set(None).await)
  );
}

#[tokio::test(start_paused = true)]
async fn shutdown() {
  let shutdown_trigger = ComponentShutdownTrigger::default();
  let batch_builder = BatchBuilder::new(
    &Collector::default().scope("test"),
    &QueuePolicy {
      queue_max_bytes: Some(6),
      ..Default::default()
    },
    TestBatch::default,
    shutdown_trigger.make_shutdown(),
  );
  shutdown_trigger.shutdown().await;
  assert_eq!(None, map_return(batch_builder.next_batch_set(None).await));
}

#[tokio::test(start_paused = true)]
async fn shutdown_with_pending_batch() {
  let shutdown_trigger = ComponentShutdownTrigger::default();
  let batch_builder = BatchBuilder::new(
    &Collector::default().scope("test"),
    &QueuePolicy {
      queue_max_bytes: Some(6),
      ..Default::default()
    },
    TestBatch::default,
    shutdown_trigger.make_shutdown(),
  );
  batch_builder.send(once(("a", expect_finish(1))));
  shutdown_trigger.shutdown().await;
  assert_eq!(
    Some(vec![vec!["a"]]),
    map_return(batch_builder.next_batch_set(None).await)
  );
  assert_eq!(None, map_return(batch_builder.next_batch_set(None).await));

  // TODO(mattklein123): Verify the following is dropped when stats are added.
  batch_builder.send(once(("b", None)));
  assert_eq!(None, map_return(batch_builder.next_batch_set(None).await));
}

#[tokio::test(start_paused = true)]
async fn overflow() {
  let shutdown_trigger = ComponentShutdownTrigger::default();
  let batch_builder = BatchBuilder::new(
    &Collector::default().scope("test"),
    &QueuePolicy {
      queue_max_bytes: Some(6),
      ..Default::default()
    },
    TestBatch::default,
    shutdown_trigger.make_shutdown(),
  );

  batch_builder.send(once(("a", None)));
  batch_builder.send(once(("b", complete(2))));
  batch_builder.send(once(("c", complete(2))));
  batch_builder.send(once(("d", None)));
  batch_builder.send(once(("e", complete(3))));
  assert_eq!(
    Some(vec![vec!["d", "e"]]),
    map_return(batch_builder.next_batch_set(Some(1)).await)
  );
  assert_eq!(
    Some(vec![vec!["c"]]),
    map_return(batch_builder.next_batch_set(Some(1)).await)
  );

  let poll_future = batch_builder.next_batch_set(Some(16));
  pin!(poll_future);
  assert_eq!(Poll::Pending, poll!(poll_future));
}

#[tokio::test(start_paused = true)]
async fn max_size_overflow() {
  let shutdown_trigger = ComponentShutdownTrigger::default();
  let batch_builder = BatchBuilder::new(
    &Collector::default().scope("test"),
    &QueuePolicy {
      queue_max_bytes: Some(6),
      ..Default::default()
    },
    TestBatch::default,
    shutdown_trigger.make_shutdown(),
  );
  batch_builder.send(once(("a", None)));
  batch_builder.send(once(("b", complete(2))));
  batch_builder.send(once(("c", None)));
  batch_builder.send(once(("d", complete(2))));
  batch_builder.send(once(("e", None)));
  batch_builder.send(once(("f", complete(7))));
  assert_eq!(
    Some(vec![vec!["c", "d"], vec!["a", "b"]]),
    map_return(batch_builder.next_batch_set(Some(16)).await)
  );
}
