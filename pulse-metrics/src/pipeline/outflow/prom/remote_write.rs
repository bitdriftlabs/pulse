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
use crate::clients::prom::{
  HyperPromRemoteWriteClient,
  PromRemoteWriteClient,
  compress_write_request,
  should_retry,
};
use crate::clients::retry::Retry;
use crate::pipeline::config::{DEFAULT_REQUEST_TIMEOUT, default_max_in_flight};
use crate::pipeline::outflow::prom::retry_offload::maybe_queue_for_retry;
use crate::pipeline::outflow::{OutflowFactoryContext, OutflowStats, PipelineOutflow};
use crate::pipeline::time::RealTimeProvider;
use crate::protos::metric::ParsedMetric;
use crate::protos::prom::{ChangedTypeTracker, MetadataType, ToWriteRequestOptions};
use async_trait::async_trait;
use axum::http::HeaderValue;
use backoff::ExponentialBackoffBuilder;
use backoff::backoff::Backoff;
use bd_log::warn_every;
use bd_server_stats::stats::{AutoGauge, Scope};
use bd_shutdown::{ComponentShutdown, ComponentStatus};
use bd_time::TimeDurationExt;
use bytes::Bytes;
use http::HeaderMap;
use prom_remote_write::PromRemoteWriteClientConfig;
use prometheus::{Histogram, IntCounter, IntGauge};
use pulse_common::proto::ProtoDurationToStdDuration;
use pulse_protobuf::protos::pulse::config::outflow::v1::prom_remote_write;
use std::sync::Arc;
use std::time::Instant;
use time::ext::NumericalDuration;
use tokio::sync::Semaphore;

const DEFAULT_BATCH_MAX_SAMPLES: u64 = 1000;

//
// PromRemoteWriteOutflowStats
//

// Stats for the Prom remote write outflow.
#[derive(Clone)]
struct PromRemoteWriteOutflowStats {
  outflow_stats: OutflowStats,
  requests_fail: IntCounter,
  requests_in_flight: IntGauge,
  requests_retry: IntCounter,
  requests_total: IntCounter,
  requests_time: Histogram,
  offload_queue_tx: IntCounter,
  offload_queue_rx: IntCounter,
}

impl PromRemoteWriteOutflowStats {
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
// PromRemoteWriteOutflow
//

// An outflow that writes to a Prom remote write capable endpoint.
pub struct PromRemoteWriteOutflow {
  name: String,
  stats: PromRemoteWriteOutflowStats,
  retry: Arc<Retry>,
  backoff: Arc<dyn Fn() -> Box<dyn Backoff + Send> + Send + Sync>,
  batch_router: Arc<dyn BatchRouter>,
  offload_queue: Option<Arc<dyn OffloadQueue>>,
  semaphore: Arc<Semaphore>,
  client: Arc<dyn PromRemoteWriteClient>,
  max_in_flight: usize,
  config: PromRemoteWriteClientConfig,
}

impl PromRemoteWriteOutflow {
  pub async fn new(
    config: PromRemoteWriteClientConfig,
    context: OutflowFactoryContext,
  ) -> anyhow::Result<Arc<Self>> {
    let request_timeout = config
      .request_timeout
      .unwrap_duration_or(DEFAULT_REQUEST_TIMEOUT);
    let client = Arc::new(
      HyperPromRemoteWriteClient::new(
        config.send_to.clone().into(),
        request_timeout,
        config.auth.clone().into_option(),
        config.request_headers.clone(),
      )
      .await?,
    );
    Self::new_with_client_and_backoff(
      context.name,
      context.stats,
      config,
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
    name: String,
    stats: OutflowStats,
    config: PromRemoteWriteClientConfig,
    client: Arc<dyn PromRemoteWriteClient>,
    backoff: Arc<dyn Fn() -> Box<dyn Backoff + Send> + Send + Sync>,
    shutdown: ComponentShutdown,
  ) -> anyhow::Result<Arc<Self>> {
    let changed_type_tracker = Arc::new(ChangedTypeTracker::new(&stats.stats));
    let batch_router = if config.lyft_specific_config.is_some() {
      Arc::new(LyftBatchRouter::new(
        &config,
        &stats.stats,
        shutdown.clone(),
        changed_type_tracker,
      )) as Arc<dyn BatchRouter>
    } else {
      Arc::new(DefaultBatchRouter::new(
        config.clone(),
        &stats.stats,
        shutdown.clone(),
        changed_type_tracker,
      )) as Arc<dyn BatchRouter>
    };

    let stats = PromRemoteWriteOutflowStats::new(stats);
    let retry = Retry::new(&config.retry_policy)?;
    let offload_queue = if let Some(queue_type) = config
      .retry_policy
      .as_ref()
      .and_then(|retry_policy| retry_policy.offload_queue.as_ref())
      .and_then(|offload_queue| offload_queue.queue_type.as_ref())
    {
      Some(create_offload_queue(queue_type).await?)
    } else {
      None
    };

    let max_in_flight = config
      .max_in_flight
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
      config,
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

      log::debug!("sending batch of {} metric(s)", num_metrics);
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
            &self.config.retry_policy.offload_queue,
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
        let PromBatch::Complete {
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
impl PipelineOutflow for PromRemoteWriteOutflow {
  async fn recv_samples(&self, samples: Vec<ParsedMetric>) {
    self.batch_router.send(samples);
  }
}

//
// BatchRouter
//

// Wraps potentially routing batches differently.
#[async_trait]
trait BatchRouter: Send + Sync {
  fn send(&self, samples: Vec<ParsedMetric>);
  async fn next_batch_set(&self, max_items: Option<usize>) -> Option<Vec<PromBatch>>;
}

//
// DefaultBatchRouter
//

// Default router which just forwards on.
struct DefaultBatchRouter {
  builder: Arc<BatchBuilder<ParsedMetric, PromBatch>>,
}

impl DefaultBatchRouter {
  fn new(
    config: PromRemoteWriteClientConfig,
    scope: &Scope,
    shutdown: ComponentShutdown,
    changed_type_tracker: Arc<ChangedTypeTracker>,
  ) -> Self {
    Self {
      builder: Self::make_batch_builder(config, scope, shutdown, None, changed_type_tracker),
    }
  }

  #[allow(clippy::needless_pass_by_value)] // Spurious
  fn make_batch_builder(
    config: PromRemoteWriteClientConfig,
    scope: &Scope,
    shutdown: ComponentShutdown,
    extra_headers: Option<Arc<HeaderMap>>,
    changed_type_tracker: Arc<ChangedTypeTracker>,
  ) -> Arc<BatchBuilder<ParsedMetric, PromBatch>> {
    let batch_max_samples: usize = config
      .batch_max_samples
      .unwrap_or(DEFAULT_BATCH_MAX_SAMPLES)
      .try_into()
      .unwrap();

    BatchBuilder::new(
      scope,
      &config.queue_policy,
      move || {
        PromBatch::new(
          batch_max_samples,
          config.metadata_only,
          config.convert_metric_name.unwrap_or(true),
          extra_headers.clone(),
          changed_type_tracker.clone(),
        )
      },
      shutdown,
    )
  }
}

#[async_trait]
impl BatchRouter for DefaultBatchRouter {
  fn send(&self, samples: Vec<ParsedMetric>) {
    self.builder.send(samples.into_iter());
  }

  async fn next_batch_set(&self, max_items: Option<usize>) -> Option<Vec<PromBatch>> {
    self.builder.next_batch_set(max_items).await
  }
}

//
// LyftBatchRouter
//

// This is a router specific to Lyft's migration away from WFP. In the future we can potentially
// replace this with generic routing code if this is useful to other customers.
struct LyftBatchRouter {
  generic: Arc<BatchBuilder<ParsedMetric, PromBatch>>,
  instance: Option<Arc<BatchBuilder<ParsedMetric, PromBatch>>>,
  cloudwatch: Option<Arc<BatchBuilder<ParsedMetric, PromBatch>>>,
}

impl LyftBatchRouter {
  fn new(
    config: &PromRemoteWriteClientConfig,
    scope: &Scope,
    shutdown: ComponentShutdown,
    changed_type_tracker: Arc<ChangedTypeTracker>,
  ) -> Self {
    let lyft_config = config.lyft_specific_config.as_ref().unwrap();
    let generic = DefaultBatchRouter::make_batch_builder(
      config.clone(),
      &scope.scope("general"),
      shutdown.clone(),
      Self::make_storage_headers(&lyft_config.general_storage_policy),
      changed_type_tracker.clone(),
    );
    let instance = lyft_config
      .instance_metrics_storage_policy
      .as_ref()
      .map(|p| {
        DefaultBatchRouter::make_batch_builder(
          config.clone(),
          &scope.scope("instance"),
          shutdown.clone(),
          Self::make_storage_headers(p),
          changed_type_tracker.clone(),
        )
      });
    let cloudwatch = lyft_config
      .cloudwatch_metrics_storage_policy
      .as_ref()
      .map(|p| {
        DefaultBatchRouter::make_batch_builder(
          config.clone(),
          &scope.scope("cloudwatch"),
          shutdown,
          Self::make_storage_headers(p),
          changed_type_tracker,
        )
      });

    Self {
      generic,
      instance,
      cloudwatch,
    }
  }

  fn make_storage_headers(storage_policy: &str) -> Option<Arc<HeaderMap>> {
    let mut header_map = HeaderMap::new();
    header_map.insert("M3-Metrics-Type", HeaderValue::from_static("aggregated"));
    header_map.insert(
      "M3-Storage-Policy",
      HeaderValue::from_str(storage_policy).unwrap(),
    );
    Some(Arc::new(header_map))
  }

  fn is_cloudwatch(name: &[u8]) -> bool {
    // Cloudwatch metrics start with <namespace>:infra:aws
    let mut name_tokens = name.split(|c| *c == b':');
    if name_tokens.next().is_none() {
      return false;
    }
    if name_tokens.next().is_none_or(|t| t != b"infra") {
      return false;
    }
    if name_tokens.next().is_none_or(|t| t != b"aws") {
      return false;
    }

    true
  }
}

#[async_trait]
impl BatchRouter for LyftBatchRouter {
  fn send(&self, samples: Vec<ParsedMetric>) {
    let mut cloudwatch = Vec::new();
    let mut instance = Vec::new();
    let mut generic = Vec::new();

    for sample in samples {
      if self.cloudwatch.is_some()
        && sample
          .metric()
          .get_id()
          .tag("source")
          .is_some_and(|v| !v.value.starts_with(b"statsd"))
        && Self::is_cloudwatch(sample.metric().get_id().name())
      {
        cloudwatch.push(sample);
      } else if self.instance.is_some() && sample.metric().get_id().tag("host").is_some() {
        instance.push(sample);
      } else {
        generic.push(sample);
      }
    }

    if !cloudwatch.is_empty() {
      self
        .cloudwatch
        .as_ref()
        .unwrap()
        .send(cloudwatch.into_iter());
    }
    if !instance.is_empty() {
      self.instance.as_ref().unwrap().send(instance.into_iter());
    }
    self.generic.send(generic.into_iter());
  }

  async fn next_batch_set(&self, max_items: Option<usize>) -> Option<Vec<PromBatch>> {
    // The following does a select over all possible batches, handling the case where either of
    // cloudwatch/instance are not configured. Each batcher will return None when it is shutdown.
    // Thus, if all branches end up disabled we should return None to indicate we are done.
    tokio::select! {
      Some(batch_set) = self.generic.next_batch_set(max_items) => Some(batch_set),
      Some(batch_set) = async {
        self.instance.as_ref().unwrap().next_batch_set(max_items).await
      }, if self.instance.is_some() => Some(batch_set),
      Some(batch_set) = async {
        self.cloudwatch.as_ref().unwrap().next_batch_set(max_items).await
      }, if self.cloudwatch.is_some() => Some(batch_set),
      else => None,
    }
  }
}

//
// PromBatch
//

// Metric batches that will be written to Prometheus remote write endpoints.
enum PromBatch {
  Building {
    samples: Vec<ParsedMetric>,
    max_samples: usize,
    metadata_only: bool,
    convert_name: bool,
    extra_headers: Option<Arc<HeaderMap>>,
    changed_type_tracker: Arc<ChangedTypeTracker>,
  },
  Complete {
    compressed_write_request: Bytes,
    received_at: Vec<Instant>,
    extra_headers: Option<Arc<HeaderMap>>,
  },
}

impl PromBatch {
  fn new(
    max_samples: usize,
    metadata_only: bool,
    convert_name: bool,
    extra_headers: Option<Arc<HeaderMap>>,
    changed_type_tracker: Arc<ChangedTypeTracker>,
  ) -> Self {
    Self::Building {
      samples: Vec::with_capacity(max_samples),
      max_samples,
      metadata_only,
      convert_name,
      extra_headers,
      changed_type_tracker,
    }
  }
}

impl Batch<ParsedMetric> for PromBatch {
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
    let (samples, metadata_only, convert_name, extra_headers, changed_type_tracker) = match self {
      Self::Building {
        samples,
        metadata_only,
        convert_name,
        extra_headers,
        changed_type_tracker,
        ..
      } => (
        std::mem::take(samples),
        *metadata_only,
        *convert_name,
        std::mem::take(extra_headers),
        changed_type_tracker,
      ),
      Self::Complete { .. } => unreachable!(),
    };

    let received_at = samples.iter().map(ParsedMetric::received_at).collect();
    let write_request = ParsedMetric::to_write_request(
      samples,
      &ToWriteRequestOptions {
        metadata: if metadata_only {
          MetadataType::Only
        } else {
          MetadataType::Normal
        },
        convert_name,
      },
      changed_type_tracker,
    );
    log::trace!("WriteRequest batched and ready to send: {}", write_request);
    let compressed_write_request = compress_write_request(&write_request);
    let size = compressed_write_request.len();
    *self = Self::Complete {
      compressed_write_request: compressed_write_request.into(),
      received_at,
      extra_headers,
    };
    size
  }
}
