// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/licenses/strict/1.0.0.txt

#[cfg(test)]
#[path = "./retry_offload_test.rs"]
mod retry_offload_test;

use crate::clients::http::HttpRemoteWriteError;
use crate::pipeline::time::TimeProvider;
use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_sdk_sqs::Client;
use aws_sdk_sqs::config::StalledStreamProtectionConfig;
use aws_sdk_sqs::types::DeleteMessageBatchRequestEntry;
use bd_log_util::warn_every;
pub use bd_otlp_metrics::{OffloadQueue, SerializedOffloadRequest};
use bd_otlp_metrics::{OffloadRetryPolicy, maybe_queue_for_retry as queue_for_retry};
use itertools::Itertools;
use pulse_common::proto::ProtoDurationToStdDuration;
use pulse_protobuf::protos::pulse::config::common::v1::retry::retry_offload_queue::Queue_type;
use pulse_protobuf::protos::pulse::config::common::v1::retry::{
  AwsSqsRetryOffloadQueue,
  RetryOffloadQueue,
};
use std::sync::Arc;
use time::Duration;
use time::ext::NumericalDuration;
use tokio::sync::{Mutex, mpsc};

// Create a concrete offload queue based on configuration.
pub async fn create_offload_queue(
  queue_type: &Queue_type,
) -> anyhow::Result<Arc<dyn OffloadQueue>> {
  match queue_type {
    Queue_type::LoopbackForTest(_) => Ok(LoopbackForTestOffloadQueue::create()),
    Queue_type::AwsSqs(aws_sqs) => AwsSqsOffloadQueue::create(aws_sqs).await,
  }
}

// Attempt to queue a failed request to the offload queue.
pub async fn maybe_queue_for_retry(
  offload_queue: Option<&Arc<dyn OffloadQueue>>,
  offload_queue_config: &RetryOffloadQueue,
  e: &HttpRemoteWriteError,
  serialized: SerializedOffloadRequest,
  time_provider: &dyn TimeProvider,
) -> bool {
  let policy = OffloadRetryPolicy {
    backoff: offload_queue_config
      .backoff
      .unwrap_duration_or(20.seconds()),
    max_send_attempts: offload_queue_config.max_send_attempts,
    window: offload_queue_config.window.unwrap_duration_or(20.minutes()),
  };
  match queue_for_retry(
    offload_queue,
    policy,
    e,
    serialized,
    time_provider.now_utc(),
  )
  .await
  {
    Ok(queued) => queued,
    Err(e) => {
      warn_every!(15.seconds(), "failed to queue to offload: {e}");
      false
    },
  }
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
  fn create() -> Arc<dyn OffloadQueue> {
    let (tx, rx) = mpsc::channel(16);

    Arc::new(Self {
      tx,
      rx: Mutex::new(rx),
    })
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
    let sdk_config = aws_config::defaults(BehaviorVersion::v2026_01_12())
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
