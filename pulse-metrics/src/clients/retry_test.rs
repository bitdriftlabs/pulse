// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::clients::retry::Retry;
use anyhow::anyhow;
use futures::poll;
use matches::assert_matches;
use pulse_protobuf::protos::pulse::config::common::v1::retry::RetryPolicy;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::Poll;
use tokio::pin;
use tokio::sync::Notify;

#[tokio::test]
async fn retry() {
  let retry = Retry::new(&RetryPolicy::default()).unwrap();

  // Successful request.
  retry
    .retry_notify(
      backoff::backoff::Zero {},
      || async { Ok::<_, backoff::Error<anyhow::Error>>(()) },
      || unreachable!(),
    )
    .await
    .unwrap();

  // Make sure we can do at least one retry with the default budget of 10%.
  let calls = Arc::new(AtomicU64::default());
  let mut retries = 0;
  retry
    .retry_notify(
      backoff::backoff::Zero {},
      || async {
        if 0 == calls.fetch_add(1, Ordering::Relaxed) {
          Err(backoff::Error::transient(anyhow!("error")))
        } else {
          Ok::<_, backoff::Error<anyhow::Error>>(())
        }
      },
      || retries += 1,
    )
    .await
    .unwrap();
  assert_eq!(1, retries);
  assert_eq!(2, calls.load(Ordering::Relaxed));
}

#[tokio::test]
async fn max_retries() {
  let retry = Retry::new(&RetryPolicy {
    max_retries: Some(0),
    ..Default::default()
  })
  .unwrap();

  let calls = Arc::new(AtomicU64::default());
  retry
    .retry_notify(
      backoff::backoff::Zero {},
      || async {
        calls.fetch_add(1, Ordering::Relaxed);
        Err::<(), _>(backoff::Error::transient(anyhow!("error")))
      },
      || unreachable!(),
    )
    .await
    .unwrap_err();
  assert_eq!(1, calls.load(Ordering::Relaxed));
}

#[tokio::test]
async fn over_budget() {
  let retry = Retry::new(&RetryPolicy::default()).unwrap();

  // Create a request with a retry that is pending until we notify it.
  let calls = Arc::new(AtomicU64::default());
  let retries = Arc::new(AtomicU64::default());
  let notify = Arc::new(Notify::new());
  let request1_future = retry.retry_notify(
    backoff::backoff::Zero {},
    || async {
      if 0 == calls.fetch_add(1, Ordering::Relaxed) {
        Err(backoff::Error::transient(anyhow!("error")))
      } else {
        notify.notified().await;
        Ok(1)
      }
    },
    || {
      retries.fetch_add(1, Ordering::Relaxed);
    },
  );
  pin!(request1_future);
  assert_matches!(poll!(request1_future.as_mut()), Poll::Pending);
  assert_eq!(1, retries.load(Ordering::Relaxed));

  // This request should not be able to retry because it is over budget.
  retry
    .retry_notify(
      backoff::backoff::Zero {},
      || async { Err::<u64, _>(backoff::Error::transient(anyhow!("error"))) },
      || unreachable!(),
    )
    .await
    .unwrap_err();

  // Now release the first request.
  notify.notify_one();
  request1_future.await.unwrap();

  // There should be budget for a retry now.
  let calls = Arc::new(AtomicU64::default());
  let mut retries = 0;
  retry
    .retry_notify(
      backoff::backoff::Zero {},
      || async {
        if 0 == calls.fetch_add(1, Ordering::Relaxed) {
          Err(backoff::Error::transient(anyhow!("error")))
        } else {
          Ok::<_, backoff::Error<anyhow::Error>>(())
        }
      },
      || retries += 1,
    )
    .await
    .unwrap();
  assert_eq!(1, retries);
  assert_eq!(2, calls.load(Ordering::Relaxed));
}
