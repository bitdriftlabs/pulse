// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::*;
use crate::test::{make_metric, processor_factory_context_for_test};
use bd_test_helpers::make_mut;
use prometheus::labels;
use tokio::join;
use tokio::sync::oneshot;

#[tokio::test]
async fn lifo() {
  let (mut helper, factory) = processor_factory_context_for_test();
  let processor = BufferProcessor::new(
    &BufferConfig {
      max_buffered_metrics: 5,
      num_consumers: 1,
      ..Default::default()
    },
    factory,
  );

  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .return_once(|samples| {
      assert_eq!(1, samples.len());
      assert_eq!("bar", samples[0].metric().get_id().name());
    });
  let (tx, rx) = oneshot::channel();
  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .return_once(|samples| {
      assert_eq!(1, samples.len());
      assert_eq!("foo", samples[0].metric().get_id().name());
      tx.send(()).unwrap();
    });
  processor
    .clone()
    .recv_samples(vec![make_metric("foo", &[], 0)])
    .await;
  processor
    .recv_samples(vec![make_metric("bar", &[], 0)])
    .await;
  rx.await.unwrap();
}

#[tokio::test]
async fn basic_flow() {
  let (mut helper, factory) = processor_factory_context_for_test();
  let processor = BufferProcessor::new(
    &BufferConfig {
      max_buffered_metrics: 5,
      num_consumers: 2,
      ..Default::default()
    },
    factory,
  );

  // A single sample should get through and then drain.
  processor
    .clone()
    .recv_samples(vec![make_metric("foo", &[], 0)])
    .await;

  let (tx, rx) = oneshot::channel();
  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .return_once(|samples| {
      assert_eq!(1, samples.len());
      tx.send(()).unwrap();
    });
  rx.await.unwrap();

  // The 2nd set of samples should get through while the 1st should be dropped (oldest).
  processor
    .clone()
    .recv_samples(vec![make_metric("foo", &[], 0); 5])
    .await;
  processor
    .clone()
    .recv_samples(vec![make_metric("bar", &[], 0); 5])
    .await;

  let (tx, rx) = oneshot::channel();
  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .return_once(|samples| {
      assert_eq!(5, samples.len());
      assert_eq!("bar", samples[0].metric().get_id().name());
      tx.send(()).unwrap();
    });
  rx.await.unwrap();

  // Make sure everything has been cleared out properly.
  processor
    .clone()
    .recv_samples(vec![make_metric("foo", &[], 0); 2])
    .await;
  processor
    .clone()
    .recv_samples(vec![make_metric("foo", &[], 0); 3])
    .await;
  let (tx, rx1) = oneshot::channel();
  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .return_once(|samples| {
      tx.send(samples.len()).unwrap();
    });
  let (tx, rx2) = oneshot::channel();
  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .return_once(|samples| {
      tx.send(samples.len()).unwrap();
    });
  let (r1, r2) = join!(rx1, rx2);
  assert_eq!(5, r1.unwrap() + r2.unwrap());

  processor
    .recv_samples(vec![make_metric("bar", &[], 0); 3])
    .await;
  let (tx, mut rx) = oneshot::channel();
  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .return_once(|samples| {
      tx.send(samples.len()).unwrap();
    });
  helper.shutdown_trigger.shutdown().await;
  assert_eq!(3, rx.try_recv().unwrap());

  helper
    .stats_helper
    .assert_counter_eq(5, "processor:dropped", &labels! {});
}
