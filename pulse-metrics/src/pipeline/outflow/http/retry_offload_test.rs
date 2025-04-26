// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::maybe_queue_for_retry;
use crate::clients::http::HttpRemoteWriteError;
use crate::pipeline::outflow::http::retry_offload::{
  MockOffloadQueue,
  OffloadQueue,
  SerializedOffloadRequest,
};
use crate::pipeline::time::TestTimeProvider;
use bd_test_helpers::make_mut;
use bd_time::ToProtoDuration;
use bytes::Bytes;
use mockall::predicate::{always, eq};
use pulse_protobuf::protos::pulse::config::common::v1::retry::RetryOffloadQueue;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use time::ext::NumericalDuration;
use tokio::sync::mpsc;

#[tokio::test]
async fn maybe_queue_for_retry_max_window() {
  let mut mock_offload_queue = Arc::new(MockOffloadQueue::new());
  let dyn_offload_queue = mock_offload_queue.clone() as Arc<dyn OffloadQueue>;
  let config = RetryOffloadQueue {
    max_send_attempts: Some(2),
    window: 1.minutes().into_proto(),
    backoff: 5.seconds().into_proto(),
    ..Default::default()
  };
  let time_provider = TestTimeProvider::default();
  let (tx, mut rx) = mpsc::channel(1);

  // First try.
  let serialized = SerializedOffloadRequest::new(&Bytes::new(), None, 5, &time_provider);
  let cloned_tx = tx.clone();
  make_mut(&mut mock_offload_queue)
    .expect_queue_write_request()
    .times(1)
    .with(always(), eq(5.seconds()))
    .return_once(move |serialized, _| {
      cloned_tx.try_send(serialized).unwrap();
      Ok(())
    });
  assert!(
    maybe_queue_for_retry(
      Some(&dyn_offload_queue),
      &config,
      &HttpRemoteWriteError::Timeout,
      serialized,
      &time_provider
    )
    .await
  );
  let serialized = rx.recv().await.unwrap();

  // Advance time, this should be above the window.
  time_provider.time.store(61, Ordering::SeqCst);
  assert!(
    !maybe_queue_for_retry(
      Some(&dyn_offload_queue),
      &config,
      &HttpRemoteWriteError::Timeout,
      serialized,
      &time_provider
    )
    .await
  );
}

#[tokio::test]
async fn maybe_queue_for_retry_max_attempts() {
  let mut mock_offload_queue = Arc::new(MockOffloadQueue::new());
  let dyn_offload_queue = mock_offload_queue.clone() as Arc<dyn OffloadQueue>;
  let config = RetryOffloadQueue {
    max_send_attempts: Some(2),
    window: 1.minutes().into_proto(),
    backoff: 5.seconds().into_proto(),
    ..Default::default()
  };
  let time_provider = TestTimeProvider::default();
  let (tx, mut rx) = mpsc::channel(1);

  // No offload queue.
  let serialized = SerializedOffloadRequest::new(&Bytes::new(), None, 5, &time_provider);
  assert!(
    !maybe_queue_for_retry(
      None,
      &config,
      &HttpRemoteWriteError::Timeout,
      serialized,
      &time_provider
    )
    .await
  );

  // First try.
  let serialized = SerializedOffloadRequest::new(&Bytes::new(), None, 5, &time_provider);
  let cloned_tx = tx.clone();
  make_mut(&mut mock_offload_queue)
    .expect_queue_write_request()
    .times(1)
    .with(always(), eq(5.seconds()))
    .return_once(move |serialized, _| {
      cloned_tx.try_send(serialized).unwrap();
      Ok(())
    });
  assert!(
    maybe_queue_for_retry(
      Some(&dyn_offload_queue),
      &config,
      &HttpRemoteWriteError::Timeout,
      serialized,
      &time_provider
    )
    .await
  );
  let serialized = rx.recv().await.unwrap();

  // Second try.
  make_mut(&mut mock_offload_queue)
    .expect_queue_write_request()
    .times(1)
    .with(always(), eq(10.seconds()))
    .return_once(move |serialized, _| {
      tx.try_send(serialized).unwrap();
      Ok(())
    });
  assert!(
    maybe_queue_for_retry(
      Some(&dyn_offload_queue),
      &config,
      &HttpRemoteWriteError::Timeout,
      serialized,
      &time_provider
    )
    .await
  );
  let serialized = rx.recv().await.unwrap();

  // Third try is above max tries.
  assert!(
    !maybe_queue_for_retry(
      Some(&dyn_offload_queue),
      &config,
      &HttpRemoteWriteError::Timeout,
      serialized,
      &time_provider
    )
    .await
  );
}
