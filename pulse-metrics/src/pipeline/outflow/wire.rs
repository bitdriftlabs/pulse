// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::{OutflowFactoryContext, OutflowStats, PipelineOutflow};
use crate::batch::{Batch, BatchBuilder};
use crate::clients::client_pool::Pool;
use crate::pipeline::config::default_max_in_flight;
use crate::protos::metric::{Metric, MetricId, MetricType, MetricValue, ParsedMetric};
use async_trait::async_trait;
use bd_log::warn_every;
use bd_server_stats::stats::AutoGauge;
use bd_shutdown::ComponentShutdown;
use itertools::Either;
use prometheus::{IntCounter, IntGauge};
use pulse_protobuf::protos::pulse::config::common::v1::common::WireProtocol;
use pulse_protobuf::protos::pulse::config::outflow::v1::wire::CommonWireClientConfig;
use std::sync::Arc;
use std::time::Instant;
use time::ext::NumericalDuration;
use tokio::net::UdpSocket;
use tokio::sync::Semaphore;

const DEFAULT_BATCH_MAX_BYTES: u64 = 8 * 1024;

#[derive(Clone)]
struct WireOutflowStats {
  outflow_stats: OutflowStats,
  outgoing_bytes: IntCounter,
  dropped_bytes: IntCounter,
  client_pool_error: IntCounter,
  client_write_error: IntCounter,
  requests_in_flight: IntGauge,
}

impl WireOutflowStats {
  fn new(outflow_stats: OutflowStats) -> Self {
    let stats = outflow_stats.stats.clone();
    Self {
      outflow_stats,
      outgoing_bytes: stats.counter("outgoing_bytes"),
      dropped_bytes: stats.counter("dropped_bytes"),
      client_pool_error: stats.counter("client_pool_error"),
      client_write_error: stats.counter("client_write_error"),
      requests_in_flight: stats.gauge("requests_in_flight"),
    }
  }
}

#[derive(Clone)]
pub(super) enum WireOutflowClient {
  Pool(Pool),
  Udp(Arc<UdpSocket>),
  Null(),
}

/// An outflow for wire protocols that use UDP, Unix, or TCP for the transport layer.
pub(super) struct WireOutflow {
  batch_builder: Arc<BatchBuilder<ParsedMetric, WireBatch>>,
}

impl WireOutflow {
  pub fn new(
    config: &CommonWireClientConfig,
    client: WireOutflowClient,
    context: OutflowFactoryContext,
  ) -> Self {
    let protocol = Arc::new(config.protocol.clone().unwrap());
    let batch_max_bytes: usize = config
      .batch_max_bytes
      .unwrap_or(DEFAULT_BATCH_MAX_BYTES)
      .try_into()
      .unwrap();
    let batch_builder = BatchBuilder::new(
      &context.stats.stats,
      &config.queue_policy,
      move || WireBatch::new(protocol.clone(), batch_max_bytes),
      context.shutdown_trigger_handle.make_shutdown(),
    );
    let stats = WireOutflowStats::new(context.stats);

    tokio::spawn(send_task(
      context.name.clone(),
      stats,
      client,
      context.shutdown_trigger_handle.make_shutdown(),
      config
        .max_in_flight
        .unwrap_or(default_max_in_flight())
        .try_into()
        .unwrap(),
      batch_builder.clone(),
    ));

    Self { batch_builder }
  }
}

#[async_trait]
impl PipelineOutflow for WireOutflow {
  async fn recv_samples(&self, samples: Vec<ParsedMetric>) {
    let samples = samples
      .into_iter()
      .filter_map(|sample| {
        // TODO(mattklein123): Support wire output for histograms and summaries. Currently we
        // block/eat here as the common output point.
        if matches!(
          sample.metric().value,
          MetricValue::Histogram(_) | MetricValue::Summary(_)
        ) {
          return None;
        }

        // In general we don't expect bulk timers to make it here given a normal configuration, but
        // if they do emit them as individual values. No effort is made to make this perform
        // well.
        if Some(MetricType::BulkTimer) == sample.metric().get_id().mtype() {
          Some(Either::Left(
            // TODO(mattklein123): Figure out if it's possible remove the to_vec() below.
            #[allow(clippy::unnecessary_to_owned)]
            sample
              .metric()
              .value
              .to_bulk_timer()
              .to_vec()
              .into_iter()
              .map(move |value| {
                ParsedMetric::new(
                  Metric::new(
                    MetricId::new(
                      sample.metric().get_id().name().clone(),
                      Some(MetricType::Timer),
                      sample.metric().get_id().tags().to_vec(),
                      true,
                    )
                    .unwrap(),
                    sample.metric().sample_rate,
                    sample.metric().timestamp,
                    MetricValue::Simple(value),
                  ),
                  sample.source().clone(),
                  sample.received_at(),
                  sample.downstream_id().clone(),
                )
              }),
          ))
        } else {
          Some(Either::Right(std::iter::once(sample)))
        }
      })
      .flatten();

    self.batch_builder.send(samples);
  }
}

struct WireBatch {
  protocol: Arc<WireProtocol>,
  max_batch_size: usize,
  payload: bytes::BytesMut,
  samples: usize,
  received_at: Vec<Instant>,
}

impl WireBatch {
  fn new(protocol: Arc<WireProtocol>, max_batch_size: usize) -> Self {
    Self {
      protocol,
      max_batch_size,
      payload: bytes::BytesMut::default(),
      samples: 0,
      received_at: Vec::default(),
    }
  }
}

impl Batch<ParsedMetric> for WireBatch {
  fn push(&mut self, samples: impl Iterator<Item = ParsedMetric>) -> Option<usize> {
    for sample in samples {
      self.received_at.push(sample.received_at());
      let line = sample.to_wire_protocol(&self.protocol);
      self.samples += 1;
      self.payload.extend_from_slice(&line);
      self.payload.extend_from_slice(b"\n");
      if self.payload.len() > self.max_batch_size {
        return Some(self.payload.len());
      }
    }
    None
  }

  fn finish(&mut self) -> usize {
    self.payload.len()
  }
}

async fn send_task(
  name: String,
  stats: WireOutflowStats,
  client: WireOutflowClient,
  shutdown: ComponentShutdown,
  max_in_flight: usize,
  batch_builder: Arc<BatchBuilder<ParsedMetric, WireBatch>>,
) {
  let semaphore = Arc::new(Semaphore::new(max_in_flight));
  let name = Arc::new(name);
  loop {
    let Some(batch_set) = batch_builder.next_batch_set(Some(max_in_flight)).await else {
      break;
    };

    for mut batch in batch_set {
      let name = name.clone();
      let stats = stats.clone();
      let client = client.clone();
      let shutdown = shutdown.clone();
      let permit = semaphore.clone().acquire_owned().await.unwrap();
      let auto_requests_in_flight = AutoGauge::new(stats.requests_in_flight.clone());
      tokio::spawn(async move {
        let bytes = batch.payload.len().try_into().unwrap();
        let samples = batch.samples.try_into().unwrap();
        let received_at = batch.received_at;

        match client {
          WireOutflowClient::Pool(pool) => {
            let mut client = match pool.get().await {
              Ok(c) => c,
              Err(e) => {
                warn_every!(
                  15.seconds(),
                  "client pool error in \"{}\" outflow: {}",
                  name,
                  e
                );
                stats.client_pool_error.inc();
                on_send_error(&stats, bytes, samples, &received_at, shutdown);
                return;
              },
            };
            if let Err(e) = client.write(&mut batch.payload).await {
              warn_every!(
                15.seconds(),
                "client write error in \"{}\" outflow: {}",
                name,
                e
              );
              stats.client_write_error.inc();
              on_send_error(&stats, bytes, samples, &received_at, shutdown);
              return;
            }
          },
          WireOutflowClient::Udp(socket) => {
            if let Err(e) = socket.send(&batch.payload).await {
              warn_every!(
                15.seconds(),
                "client write error in \"{}\" outflow: {}",
                name,
                e
              );
              stats.client_write_error.inc();
              on_send_error(&stats, bytes, samples, &received_at, shutdown);
              return;
            }
          },
          WireOutflowClient::Null() => {},
        }
        stats.outgoing_bytes.inc_by(bytes);
        stats
          .outflow_stats
          .messages_outgoing_success
          .inc_by(samples);
        stats.outflow_stats.messages_e2e_timer_observe(&received_at);
        drop(shutdown);
        drop(permit);
        drop(auto_requests_in_flight);
      });
    }
  }
}

fn on_send_error(
  stats: &WireOutflowStats,
  bytes: u64,
  samples: u64,
  received_at: &[Instant],
  shutdown: ComponentShutdown,
) {
  stats.dropped_bytes.inc_by(bytes);
  stats.outflow_stats.messages_outgoing_failed.inc_by(samples);
  stats.outflow_stats.messages_e2e_timer_observe(received_at);
  drop(shutdown);
}
