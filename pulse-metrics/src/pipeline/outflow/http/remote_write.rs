// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./remote_write_test.rs"]
mod remote_write_test;

use super::retry_offload::{OffloadQueue, SerializedOffloadRequest, create_offload_queue};
use crate::batch::{Batch, BatchBuilder};
use crate::clients::http::{
  HttpRemoteWriteClient,
  HyperHttpRemoteWriteClient,
  PROM_REMOTE_WRITE_HEADERS,
  should_retry,
};
use crate::clients::retry::Retry;
use crate::pipeline::config::{DEFAULT_REQUEST_TIMEOUT, default_max_in_flight};
use crate::pipeline::outflow::http::retry_offload::maybe_queue_for_retry;
use crate::pipeline::outflow::{OutflowFactoryContext, OutflowStats, PipelineOutflow};
use crate::pipeline::time::RealTimeProvider;
use crate::protos::metric::ParsedMetric;
use async_trait::async_trait;
use backoff::ExponentialBackoffBuilder;
use backoff::backoff::Backoff;
use bd_log::warn_every;
use bd_server_stats::stats::{AutoGauge, Scope};
use bd_shutdown::{ComponentShutdown, ComponentStatus};
use bd_time::TimeDurationExt;
use bytes::Bytes;
use http::HeaderMap;
use prometheus::{Histogram, IntCounter, IntGauge};
use protobuf::MessageField;
use protobuf::well_known_types::duration::Duration;
use pulse_common::proto::ProtoDurationToStdDuration;
use pulse_protobuf::protos::pulse::config::common::v1::retry::RetryPolicy;
use pulse_protobuf::protos::pulse::config::outflow::v1::outflow_common::{
  HttpRemoteWriteAuthConfig,
  RequestHeader,
};
use pulse_protobuf::protos::pulse::config::outflow::v1::queue_policy::QueuePolicy;
use std::sync::Arc;
use std::time::Instant;
use time::ext::NumericalDuration;
use tokio::sync::Semaphore;

const DEFAULT_BATCH_MAX_SAMPLES: u64 = 1000;

//
// HttpRemoteWriteOutflowStats
//

#[derive(Clone)]
struct HttpRemoteWriteOutflowStats {
  outflow_stats: OutflowStats,
  requests_fail: IntCounter,
  requests_in_flight: IntGauge,
  requests_retry: IntCounter,
  requests_total: IntCounter,
  requests_time: Histogram,
  offload_queue_tx: IntCounter,
  offload_queue_rx: IntCounter,
}

impl HttpRemoteWriteOutflowStats {
  fn new(outflow_stats: OutflowStats) -> Self {
    let stats = outflow_stats.stats.clone();
    Self {
      outflow_stats,
      requests_fail: stats.counter("requests_fail"),
      requests_in_flight: stats.gauge("requests_in_flight"),
      requests_retry: stats.counter("requests_retry"),
      requests_total: stats.counter("requests_total"),
      requests_time: stats.histogram("requests_time"),
      offload_queue_tx: stats.counter("offload_queue_tx"),
      offload_queue_rx: stats.counter("offload_queue_rx"),
    }
  }
}

//
// SendRequest
//

enum SendRequest {
  // Request coming from the standard inflow/batch system.
  Normal {
    compressed_write_request: Bytes,
    received_at: Vec<Instant>,
    extra_headers: Option<Arc<HeaderMap>>,
  },
  // Request that is being retried off the offload queue.
  OffloadQueue {
    serialized: SerializedOffloadRequest,
  },
}

//
// HttpRemoteWriteOutflow
//

// An outflow that writes to an HTTP remote write capable endpoint.
pub struct HttpRemoteWriteOutflow {
  name: String,
  stats: HttpRemoteWriteOutflowStats,
  retry: Arc<Retry>,
  backoff: Arc<dyn Fn() -> Box<dyn Backoff + Send> + Send + Sync>,
  batch_router: Arc<dyn BatchRouter>,
  offload_queue: Option<Arc<dyn OffloadQueue>>,
  semaphore: Arc<Semaphore>,
  client: Arc<dyn HttpRemoteWriteClient>,
  max_in_flight: usize,
  retry_policy: RetryPolicy,
}

impl HttpRemoteWriteOutflow {
  pub(crate) async fn new(
    request_timeout: MessageField<Duration>,
    retry_policy: RetryPolicy,
    max_in_flight: Option<u64>,
    batch_router: Arc<dyn BatchRouter>,
    send_to: String,
    auth_config: Option<HttpRemoteWriteAuthConfig>,
    request_headers: Vec<RequestHeader>,
    context: OutflowFactoryContext,
  ) -> anyhow::Result<Arc<Self>> {
    let request_timeout = request_timeout.unwrap_duration_or(DEFAULT_REQUEST_TIMEOUT);
    let client = Arc::new(
      HyperHttpRemoteWriteClient::new(
        send_to,
        request_timeout,
        auth_config,
        PROM_REMOTE_WRITE_HEADERS,
        request_headers,
      )
      .await?,
    );
    Self::new_with_client_and_backoff(
      retry_policy,
      max_in_flight,
      batch_router,
      context.name,
      context.stats,
      client,
      Arc::new(move || {
        Box::new(
          ExponentialBackoffBuilder::new()
            .with_max_elapsed_time(Some(request_timeout.unsigned_abs()))
            .build(),
        )
      }),
      context.shutdown_trigger_handle.make_shutdown(),
    )
    .await
  }

  async fn new_with_client_and_backoff(
    retry_policy: RetryPolicy,
    max_in_flight: Option<u64>,
    batch_router: Arc<dyn BatchRouter>,
    name: String,
    stats: OutflowStats,
    client: Arc<dyn HttpRemoteWriteClient>,
    backoff: Arc<dyn Fn() -> Box<dyn Backoff + Send> + Send + Sync>,
    shutdown: ComponentShutdown,
  ) -> anyhow::Result<Arc<Self>> {
    let stats = HttpRemoteWriteOutflowStats::new(stats);
    let retry = Retry::new(&retry_policy)?;
    let offload_queue = if let Some(queue_type) = retry_policy
      .offload_queue
      .as_ref()
      .and_then(|offload_queue| offload_queue.queue_type.as_ref())
    {
      Some(create_offload_queue(queue_type).await?)
    } else {
      None
    };

    let max_in_flight = max_in_flight
      .unwrap_or(default_max_in_flight())
      .try_into()
      .unwrap();
    let semaphore = Arc::new(Semaphore::new(max_in_flight));

    let outflow = Arc::new(Self {
      name,
      stats,
      retry,
      backoff,
      batch_router,
      offload_queue,
      semaphore,
      client,
      max_in_flight,
      retry_policy,
    });

    let cloned_outflow = outflow.clone();
    let cloned_shutdown = shutdown.clone();
    tokio::spawn(async move { cloned_outflow.send_task(cloned_shutdown).await });
    if outflow.offload_queue.is_some() {
      let cloned_outflow = outflow.clone();
      tokio::spawn(async move { cloned_outflow.offload_queue_task(shutdown).await });
    }

    Ok(outflow)
  }

  async fn send_request(self: Arc<Self>, send_request: SendRequest, shutdown: ComponentShutdown) {
    let permit = self.semaphore.clone().acquire_owned().await.unwrap();
    let auto_requests_in_flight = AutoGauge::new(self.stats.requests_in_flight.clone());

    tokio::spawn(async move {
      let (compressed_write_request, num_metrics, received_at, extra_headers, serialized) =
        match send_request {
          SendRequest::Normal {
            compressed_write_request,
            received_at,
            extra_headers,
          } => (
            compressed_write_request,
            received_at.len().try_into().unwrap(),
            Some(received_at),
            extra_headers,
            None,
          ),
          SendRequest::OffloadQueue { serialized } => (
            serialized.compressed_write_request(),
            serialized.num_metrics(),
            None,
            serialized.extra_headers(),
            Some(serialized),
          ),
        };

      log::debug!("sending batch of {num_metrics} metric(s)");
      let time = self.stats.requests_time.start_timer();
      let res = self
        .retry
        .retry_notify(
          (self.backoff)(),
          || async {
            match self
              .client
              .send_write_request(
                compressed_write_request.clone(),
                extra_headers.as_ref().map(std::convert::AsRef::as_ref),
              )
              .await
            {
              Ok(()) => Ok(()),
              Err(e) => {
                // Skip retries if shutdown is pending.
                if should_retry(&e)
                  && shutdown.component_status() != ComponentStatus::PendingShutdown
                {
                  Err(backoff::Error::transient(e))
                } else {
                  Err(backoff::Error::permanent(e))
                }
              },
            }
          },
          || {
            self.stats.requests_retry.inc();
          },
        )
        .await;

      drop(time);
      self.stats.requests_total.inc();

      // TODO(mattklein123): We don't attempt to serialize received_at for offload given it's an
      // edge case. Perhaps we should do this?
      if let Some(received_at) = received_at {
        self
          .stats
          .outflow_stats
          .messages_e2e_timer_observe(&received_at);
      }

      match res {
        Ok(()) => {
          self
            .stats
            .outflow_stats
            .messages_outgoing_success
            .inc_by(num_metrics);
        },
        Err(e) => {
          // This is incremented whether we offload or not, so that we get accurate SR for
          // upstream. Alarming should happen on drops.
          self.stats.requests_fail.inc();

          if maybe_queue_for_retry(
            self.offload_queue.as_ref(),
            &self.retry_policy.offload_queue,
            &e,
            serialized.unwrap_or_else(|| {
              SerializedOffloadRequest::new(
                &compressed_write_request,
                extra_headers,
                num_metrics,
                &RealTimeProvider {},
              )
            }),
            &RealTimeProvider {},
          )
          .await
          {
            self.stats.offload_queue_tx.inc();
            log::debug!("request sent to offload queue");
            return;
          }

          self
            .stats
            .outflow_stats
            .messages_outgoing_failed
            .inc_by(num_metrics);
          warn_every!(
            15.seconds(),
            "prometheus remote write request failed: size={}, outflow=\"{}\": {}",
            compressed_write_request.len(),
            self.name,
            e
          );
        },
      }

      drop(shutdown);
      drop(permit);
      drop(auto_requests_in_flight);
    });
  }

  async fn offload_queue_task(self: Arc<Self>, mut shutdown: ComponentShutdown) {
    let cloned_shutdown = shutdown.clone();
    let shutdown_future = shutdown.cancelled();
    tokio::pin!(shutdown_future);

    loop {
      tokio::select! {
        () = &mut shutdown_future => break,
        received = self.offload_queue.as_ref().unwrap().receive_write_requests() => {
          self.clone().process_received_serialized_requests(
            received, cloned_shutdown.clone()).await;
        }
      }
    }

    drop(cloned_shutdown);
  }

  async fn process_received_serialized_requests(
    self: Arc<Self>,
    received: anyhow::Result<Vec<SerializedOffloadRequest>>,
    shutdown: ComponentShutdown,
  ) {
    let serialized_requests = match received {
      Ok(serialized_requests) => serialized_requests,
      Err(e) => {
        warn_every!(15.seconds(), "failed to received from offload queue: {}", e);
        1.seconds().sleep().await;
        return;
      },
    };

    log::debug!(
      "received {} request(s) from offload queue",
      serialized_requests.len()
    );
    self
      .stats
      .offload_queue_rx
      .inc_by(serialized_requests.len().try_into().unwrap());
    for serialized in serialized_requests {
      self
        .clone()
        .send_request(SendRequest::OffloadQueue { serialized }, shutdown.clone())
        .await;
    }
  }

  // Task used to forward batches to the remote client.
  async fn send_task(self: Arc<Self>, shutdown: ComponentShutdown) {
    loop {
      let Some(batch_set) = self
        .batch_router
        .next_batch_set(Some(self.max_in_flight))
        .await
      else {
        return;
      };

      for batch in batch_set {
        let HttpBatch::Complete {
          compressed_write_request,
          received_at,
          extra_headers,
        } = batch
        else {
          unreachable!()
        };

        log::debug!("processing batch of {} metric(s)", received_at.len());
        self
          .clone()
          .send_request(
            SendRequest::Normal {
              compressed_write_request,
              extra_headers,
              received_at,
            },
            shutdown.clone(),
          )
          .await;
      }
    }
  }
}

#[async_trait]
impl PipelineOutflow for HttpRemoteWriteOutflow {
  async fn recv_samples(&self, samples: Vec<ParsedMetric>) {
    self.batch_router.send(samples);
  }
}

//
// BatchRouter
//

// Wraps potentially routing batches differently.
#[async_trait]
pub trait BatchRouter: Send + Sync {
  fn send(&self, samples: Vec<ParsedMetric>);
  async fn next_batch_set(&self, max_items: Option<usize>) -> Option<Vec<HttpBatch>>;
}

//
// DefaultBatchRouter
//

// Default router which just forwards on.
pub struct DefaultBatchRouter {
  builder: Arc<BatchBuilder<ParsedMetric, HttpBatch>>,
}

impl DefaultBatchRouter {
  pub(crate) fn new(
    batch_max_samples: Option<u64>,
    queue_policy: &QueuePolicy,
    scope: &Scope,
    shutdown: ComponentShutdown,
    finisher: Arc<dyn Fn(Vec<ParsedMetric>) -> Bytes + Send + Sync>,
  ) -> Self {
    Self {
      builder: Self::make_batch_builder(
        batch_max_samples,
        queue_policy,
        scope,
        shutdown,
        None,
        finisher,
      ),
    }
  }

  #[allow(clippy::needless_pass_by_value)] // Spurious
  pub(crate) fn make_batch_builder(
    batch_max_samples: Option<u64>,
    queue_policy: &QueuePolicy,
    scope: &Scope,
    shutdown: ComponentShutdown,
    extra_headers: Option<Arc<HeaderMap>>,
    finisher: Arc<dyn Fn(Vec<ParsedMetric>) -> Bytes + Send + Sync>,
  ) -> Arc<BatchBuilder<ParsedMetric, HttpBatch>> {
    let batch_max_samples: usize = batch_max_samples
      .unwrap_or(DEFAULT_BATCH_MAX_SAMPLES)
      .try_into()
      .unwrap();

    BatchBuilder::new(
      scope,
      queue_policy,
      move || HttpBatch::new(batch_max_samples, extra_headers.clone(), finisher.clone()),
      shutdown,
    )
  }
}

#[async_trait]
impl BatchRouter for DefaultBatchRouter {
  fn send(&self, samples: Vec<ParsedMetric>) {
    self.builder.send(samples.into_iter());
  }

  async fn next_batch_set(&self, max_items: Option<usize>) -> Option<Vec<HttpBatch>> {
    self.builder.next_batch_set(max_items).await
  }
}

//
// HttpBatch
//

// Metric batches that will be written to Prometheus remote write endpoints.
pub enum HttpBatch {
  Building {
    samples: Vec<ParsedMetric>,
    max_samples: usize,
    extra_headers: Option<Arc<HeaderMap>>,
    finisher: Arc<dyn Fn(Vec<ParsedMetric>) -> Bytes + Send + Sync>,
  },
  Complete {
    compressed_write_request: Bytes,
    received_at: Vec<Instant>,
    extra_headers: Option<Arc<HeaderMap>>,
  },
}

impl HttpBatch {
  fn new(
    max_samples: usize,
    extra_headers: Option<Arc<HeaderMap>>,
    finisher: Arc<dyn Fn(Vec<ParsedMetric>) -> Bytes + Send + Sync>,
  ) -> Self {
    Self::Building {
      samples: Vec::with_capacity(max_samples),
      max_samples,
      extra_headers,
      finisher,
    }
  }
}

impl Batch<ParsedMetric> for HttpBatch {
  fn push(&mut self, items: impl Iterator<Item = ParsedMetric>) -> Option<usize> {
    let (samples, max_samples) = match self {
      Self::Building {
        samples,
        max_samples,
        ..
      } => (samples, max_samples),
      Self::Complete { .. } => unreachable!(),
    };

    samples.extend(items.take(*max_samples - samples.len()));
    if samples.len() == *max_samples {
      return Some(self.finish());
    }
    None
  }

  fn finish(&mut self) -> usize {
    let (samples, extra_headers, finisher) = match self {
      Self::Building {
        samples,
        extra_headers,
        finisher,
        ..
      } => (
        std::mem::take(samples),
        std::mem::take(extra_headers),
        finisher,
      ),
      Self::Complete { .. } => unreachable!(),
    };

    let received_at = samples.iter().map(ParsedMetric::received_at).collect();
    let compressed_write_request = (finisher)(samples);
    let size = compressed_write_request.len();
    *self = Self::Complete {
      compressed_write_request,
      received_at,
      extra_headers,
    };
    size
  }
}
