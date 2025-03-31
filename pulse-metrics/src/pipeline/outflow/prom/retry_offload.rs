// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./retry_offload_test.rs"]
mod retry_offload_test;

use crate::clients::prom::{PromRemoteWriteError, should_retry};
use crate::pipeline::time::TimeProvider;
use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_sqs::Client;
use aws_sdk_sqs::config::StalledStreamProtectionConfig;
use aws_sdk_sqs::types::DeleteMessageBatchRequestEntry;
use base64ct::{Base64Unpadded, Encoding};
use bd_log::warn_every;
use bytes::Bytes;
use http::HeaderMap;
use itertools::Itertools;
use pulse_common::proto::ProtoDurationToStdDuration;
use pulse_protobuf::protos::pulse::config::common::v1::retry::retry_offload_queue::Queue_type;
use pulse_protobuf::protos::pulse::config::common::v1::retry::{
  AwsSqsRetryOffloadQueue,
  RetryOffloadQueue,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use time::ext::NumericalDuration;
use time::{Duration, OffsetDateTime};
use tokio::sync::{Mutex, mpsc};

//
// SerializedOffloadRequest
//

#[derive(Serialize, Deserialize)]
pub struct SerializedOffloadRequest {
  base64_compressed_write_request: String,
  extra_headers: Option<HashMap<String, String>>,
  num_metrics: u64,
  retry_attempt: u64,
  created_at_unix: i64,
}

impl SerializedOffloadRequest {
  pub fn new(
    compressed_write_request: &Bytes,
    extra_headers: Option<Arc<HeaderMap>>,
    num_metrics: u64,
    time_provider: &dyn TimeProvider,
  ) -> Self {
    Self {
      base64_compressed_write_request: Base64Unpadded::encode_string(compressed_write_request),
      extra_headers: extra_headers.map(|headers| {
        headers
          .iter()
          .map(|(key, value)| (key.to_string(), value.to_str().unwrap().to_string()))
          .collect()
      }),
      num_metrics,
      retry_attempt: 0,
      created_at_unix: time_provider.unix_now(),
    }
  }

  pub fn compressed_write_request(&self) -> Bytes {
    // TODO(mattklein123): Technically this can fail if incoming data is corrupted.
    Base64Unpadded::decode_vec(&self.base64_compressed_write_request)
      .unwrap()
      .into()
  }

  pub const fn num_metrics(&self) -> u64 {
    self.num_metrics
  }

  pub fn extra_headers(&self) -> Option<Arc<HeaderMap>> {
    // TODO(mattklein123): Technically this can fail if incoming data is corrupted.
    self
      .extra_headers
      .as_ref()
      .map(|extra_headers| Arc::new(extra_headers.try_into().unwrap()))
  }

  pub fn inc_retry_attempts(&mut self) {
    self.retry_attempt += 1;
  }

  pub const fn retry_attempts(&self) -> u64 {
    self.retry_attempt
  }

  pub fn created_at(&self) -> OffsetDateTime {
    // TODO(mattklein123): Technically this can fail if incoming data is corrupted.
    OffsetDateTime::from_unix_timestamp(self.created_at_unix).unwrap()
  }
}

//
// OffloadQueue
//

// Trait that abstracts an offload queue implementation.
#[mockall::automock]
#[async_trait]
pub trait OffloadQueue: Send + Sync {
  // Queue a write request for later retry.
  async fn queue_write_request(
    &self,
    serialized_request: SerializedOffloadRequest,
    backoff: Duration,
  ) -> anyhow::Result<()>;

  // Receive write requests to be retried.
  async fn receive_write_requests(&self) -> anyhow::Result<Vec<SerializedOffloadRequest>>;
}

// Create a concrete offload queue based on configuration.
pub async fn create_offload_queue(
  queue_type: &Queue_type,
) -> anyhow::Result<Arc<dyn OffloadQueue>> {
  match queue_type {
    Queue_type::LoopbackForTest(_) => LoopbackForTestOffloadQueue::create(),
    Queue_type::AwsSqs(aws_sqs) => AwsSqsOffloadQueue::create(aws_sqs).await,
  }
}

// Attempt to queue a failed request to the offload queue.
pub async fn maybe_queue_for_retry(
  offload_queue: Option<&Arc<dyn OffloadQueue>>,
  offload_queue_config: &RetryOffloadQueue,
  e: &PromRemoteWriteError,
  mut serialized: SerializedOffloadRequest,
  time_provider: &dyn TimeProvider,
) -> bool {
  let Some(offload_queue) = offload_queue else {
    return false;
  };

  if !should_retry(e) {
    log::debug!("dropping due to not retriable");
    return false;
  }

  serialized.inc_retry_attempts();
  if offload_queue_config
    .max_send_attempts
    .is_some_and(|max_send_attempts| serialized.retry_attempts() > u64::from(max_send_attempts))
  {
    log::debug!("dropping due to max attempts");
    return false;
  }

  if serialized.retry_attempts() > 1 {
    let window = offload_queue_config.window.unwrap_duration_or(20.minutes());
    if time_provider.now_utc() - serialized.created_at() > window {
      log::debug!("dropping due to max window");
      return false;
    }
  }

  let backoff = offload_queue_config
    .backoff
    .unwrap_duration_or(20.seconds())
    * (serialized.retry_attempts() as u32);

  if let Err(e) = offload_queue.queue_write_request(serialized, backoff).await {
    warn_every!(15.seconds(), "failed to queue to offload: {}", e);
    return false;
  }

  true
}

//
// LoopbackForTestQueue
//

// An implementation of OffloadQueue that exists solely for integration testing.
// TODO(mattklein123): It would be nice if this didn't require real config but right now there
// is no easy to way thread a factory all the way through. We can consider fixing this later.
struct LoopbackForTestOffloadQueue {
  tx: mpsc::Sender<SerializedOffloadRequest>,
  rx: Mutex<mpsc::Receiver<SerializedOffloadRequest>>,
}

impl LoopbackForTestOffloadQueue {
  fn create() -> anyhow::Result<Arc<dyn OffloadQueue>> {
    let (tx, rx) = mpsc::channel(16);

    Ok(Arc::new(Self {
      tx,
      rx: Mutex::new(rx),
    }))
  }
}

#[async_trait]
impl OffloadQueue for LoopbackForTestOffloadQueue {
  async fn queue_write_request(
    &self,
    serialized_request: SerializedOffloadRequest,
    _backoff: Duration,
  ) -> anyhow::Result<()> {
    self.tx.send(serialized_request).await.unwrap();
    Ok(())
  }

  async fn receive_write_requests(&self) -> anyhow::Result<Vec<SerializedOffloadRequest>> {
    Ok(vec![self.rx.lock().await.recv().await.unwrap()])
  }
}

//
// AwsSqsOffloadQueue
//

// An implementation of offload queue that uses AWS SQS.
struct AwsSqsOffloadQueue {
  client: Client,
  queue_url: String,
}

impl AwsSqsOffloadQueue {
  async fn create(config: &AwsSqsRetryOffloadQueue) -> anyhow::Result<Arc<dyn OffloadQueue>> {
    // Turn off stale protection for long polls.
    let sdk_config = aws_config::defaults(BehaviorVersion::v2025_01_17())
      .stalled_stream_protection(StalledStreamProtectionConfig::disabled())
      .load()
      .await;
    let client = Client::new(&sdk_config);
    log::info!("looking up SQS queue URL for: {}", config.queue_name);
    let queue_url = client
      .get_queue_url()
      .queue_name(config.queue_name.as_str())
      .send()
      .await?
      .queue_url()
      .unwrap()
      .to_string();
    log::info!("found queue URL: {queue_url}");

    Ok(Arc::new(Self { client, queue_url }))
  }
}

#[async_trait]
impl OffloadQueue for AwsSqsOffloadQueue {
  async fn queue_write_request(
    &self,
    serialized_request: SerializedOffloadRequest,
    backoff: Duration,
  ) -> anyhow::Result<()> {
    self
      .client
      .send_message()
      .queue_url(&self.queue_url)
      .set_delay_seconds(Some(backoff.whole_seconds().try_into().unwrap()))
      .message_body(serde_json::to_string(&serialized_request).unwrap())
      .send()
      .await?;

    Ok(())
  }

  async fn receive_write_requests(&self) -> anyhow::Result<Vec<SerializedOffloadRequest>> {
    loop {
      log::debug!("starting SQS receive");
      // According to the docs, a maximum of 10 messages can be received.
      let result = self
        .client
        .receive_message()
        .queue_url(&self.queue_url)
        .visibility_timeout(30)
        .max_number_of_messages(10)
        .wait_time_seconds(20)
        .send()
        .await?;

      if result.messages().is_empty() {
        log::debug!("no messages received from SQS. Long polling again");
        continue;
      }

      // TODO(mattklein123): It would be better to delete from the queue after processing is
      // actually complete, but in the interest of time and to match existing behavior we delete
      // now. We can return to this later.
      self
        .client
        .delete_message_batch()
        .queue_url(&self.queue_url)
        .set_entries(Some(
          result
            .messages()
            .iter()
            .enumerate()
            .map(|(index, message)| {
              DeleteMessageBatchRequestEntry::builder()
                .id(index.to_string())
                .receipt_handle(message.receipt_handle.as_ref().unwrap())
                .build()
                .unwrap()
            })
            .collect(),
        ))
        .send()
        .await?;

      return result
        .messages()
        .iter()
        .map(|message| {
          let payload: SerializedOffloadRequest = serde_json::from_str(message.body().unwrap())?;
          Ok(payload)
        })
        .try_collect();
    }
  }
}
