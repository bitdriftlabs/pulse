// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::http::remote_write::{
  BatchRouter,
  DefaultBatchRouter,
  HttpBatch,
  HttpRemoteWriteOutflow,
};
use super::{OutflowFactoryContext, OutflowStats};
use crate::batch::BatchBuilder;
use crate::clients::http::PROM_REMOTE_WRITE_HEADERS;
use crate::protos::metric::ParsedMetric;
use crate::protos::prom::{ChangedTypeTracker, MetadataType, ToWriteRequestOptions};
use async_trait::async_trait;
use bd_proto::protos::prometheus::prompb::remote::WriteRequest;
use bd_server_stats::stats::Scope;
use bd_shutdown::ComponentShutdown;
use bytes::Bytes;
use http::{HeaderMap, HeaderValue};
use prom_remote_write::prom_remote_write_client_config::LyftSpecificConfig;
use protobuf::Message;
use pulse_protobuf::protos::pulse::config::outflow::v1::prom_remote_write::{
  self,
  PromRemoteWriteClientConfig,
};
use pulse_protobuf::protos::pulse::config::outflow::v1::queue_policy::QueuePolicy;
use std::sync::Arc;

pub fn compress_write_request(write_request: &WriteRequest) -> Vec<u8> {
  let proto_encoded = write_request.write_to_bytes().unwrap();
  let proto_compressed = snap::raw::Encoder::new()
    .compress_vec(&proto_encoded)
    .unwrap();
  log::debug!(
    "compressed WriteRequest {} bytes to {} bytes",
    proto_encoded.len(),
    proto_compressed.len()
  );
  proto_compressed
}

pub fn make_prom_batch_router(
  config: &PromRemoteWriteClientConfig,
  stats: &OutflowStats,
  shutdown: ComponentShutdown,
) -> Arc<dyn BatchRouter> {
  let changed_type_tracker = Arc::new(ChangedTypeTracker::new(&stats.stats));
  let metadata_only = config.metadata_only;
  let convert_name = config.convert_metric_name.unwrap_or(true);
  let finisher = Arc::new(move |samples| {
    finish_prom_batch(samples, &changed_type_tracker, metadata_only, convert_name)
  });

  if config.lyft_specific_config.is_some() {
    Arc::new(LyftBatchRouter::new(
      config.batch_max_samples,
      &config.queue_policy,
      &config.lyft_specific_config,
      &stats.stats,
      shutdown,
      finisher,
    )) as Arc<dyn BatchRouter>
  } else {
    Arc::new(DefaultBatchRouter::new(
      config.batch_max_samples,
      &config.queue_policy,
      &stats.stats,
      shutdown,
      None,
      finisher,
    )) as Arc<dyn BatchRouter>
  }
}

pub async fn make_prom_outflow(
  config: PromRemoteWriteClientConfig,
  context: OutflowFactoryContext,
) -> anyhow::Result<Arc<HttpRemoteWriteOutflow>> {
  let batch_router = make_prom_batch_router(
    &config,
    &context.stats,
    context.shutdown_trigger_handle.make_shutdown(),
  );
  HttpRemoteWriteOutflow::new(
    config.request_timeout,
    config.retry_policy.unwrap_or_default(),
    config.max_in_flight,
    batch_router,
    config.send_to.to_string(),
    config.auth.into_option(),
    PROM_REMOTE_WRITE_HEADERS,
    config.request_headers,
    context,
  )
  .await
}

fn finish_prom_batch(
  samples: Vec<ParsedMetric>,
  changed_type_tracker: &ChangedTypeTracker,
  metadata_only: bool,
  convert_name: bool,
) -> Bytes {
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
  log::trace!("WriteRequest batched and ready to send: {write_request}");
  let compressed_write_request = compress_write_request(&write_request);
  compressed_write_request.into()
}

//
// LyftBatchRouter
//

// This is a router specific to Lyft's migration away from WFP. In the future we can potentially
// replace this with generic routing code if this is useful to other customers.
struct LyftBatchRouter {
  generic: Arc<BatchBuilder<ParsedMetric, HttpBatch>>,
  instance: Option<Arc<BatchBuilder<ParsedMetric, HttpBatch>>>,
  cloudwatch: Option<Arc<BatchBuilder<ParsedMetric, HttpBatch>>>,
}

impl LyftBatchRouter {
  fn new(
    batch_max_samples: Option<u64>,
    queue_policy: &QueuePolicy,
    lyft_config: &LyftSpecificConfig,
    scope: &Scope,
    shutdown: ComponentShutdown,
    finisher: Arc<dyn Fn(Vec<ParsedMetric>) -> Bytes + Send + Sync>,
  ) -> Self {
    let generic = DefaultBatchRouter::make_batch_builder(
      batch_max_samples,
      queue_policy,
      &scope.scope("general"),
      shutdown.clone(),
      Some(Self::make_storage_headers(
        &lyft_config.general_storage_policy,
      )),
      finisher.clone(),
    );
    let instance = lyft_config
      .instance_metrics_storage_policy
      .as_ref()
      .map(|p| {
        DefaultBatchRouter::make_batch_builder(
          batch_max_samples,
          queue_policy,
          &scope.scope("instance"),
          shutdown.clone(),
          Some(Self::make_storage_headers(p)),
          finisher.clone(),
        )
      });
    let cloudwatch = lyft_config
      .cloudwatch_metrics_storage_policy
      .as_ref()
      .map(|p| {
        DefaultBatchRouter::make_batch_builder(
          batch_max_samples,
          queue_policy,
          &scope.scope("cloudwatch"),
          shutdown,
          Some(Self::make_storage_headers(p)),
          finisher,
        )
      });

    Self {
      generic,
      instance,
      cloudwatch,
    }
  }

  fn make_storage_headers(storage_policy: &str) -> Arc<HeaderMap> {
    let mut header_map = HeaderMap::new();
    header_map.insert("M3-Metrics-Type", HeaderValue::from_static("aggregated"));
    header_map.insert(
      "M3-Storage-Policy",
      HeaderValue::from_str(storage_policy).unwrap(),
    );
    Arc::new(header_map)
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

  async fn next_batch_set(&self, max_items: Option<usize>) -> Option<Vec<HttpBatch>> {
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
