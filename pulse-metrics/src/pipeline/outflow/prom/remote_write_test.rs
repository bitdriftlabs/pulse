// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::*;
use crate::clients::prom::{MockPromRemoteWriteClient, PromRemoteWriteError};
use crate::protos::metric::DownstreamId;
use crate::test::{TestDownstreamIdProvider, make_carbon_wire_protocol};
use bd_proto::protos::prometheus::prompb::remote::WriteRequest;
use bd_server_stats::stats::Collector;
use bd_shutdown::ComponentShutdownTrigger;
use bd_time::ToProtoDuration;
use http::StatusCode;
use prom_remote_write::prom_remote_write_server_config::ParseConfig;
use protobuf::Message;
use pulse_protobuf::protos::pulse::config::inflow::v1::prom_remote_write;
use pulse_protobuf::protos::pulse::config::outflow::v1::queue_policy::QueuePolicy;
use std::time::Duration;
use time::ext::{NumericalDuration, NumericalStdDuration};
use tokio::sync::mpsc;
use tokio::time::timeout;

async fn setup_test(
  client: MockPromRemoteWriteClient,
) -> (Arc<PromRemoteWriteOutflow>, ComponentShutdownTrigger) {
  let config = PromRemoteWriteClientConfig {
    max_in_flight: Some(1),
    send_to: "http://localhost/api/v1/prom/write".into(),
    request_timeout: 10.seconds().into_proto(),
    queue_policy: Some(QueuePolicy {
      batch_fill_wait: 5.seconds().into_proto(),
      queue_max_bytes: Some(u64::MAX),
      ..Default::default()
    })
    .into(),
    batch_max_samples: Some(5),
    metadata_only: false,
    auth: None.into(),
    ..Default::default()
  };

  let scope = Collector::default().scope("stats");
  let outflow_stats = OutflowStats::new(&scope, "test");

  let shutdown_trigger = ComponentShutdownTrigger::default();

  let outflow = PromRemoteWriteOutflow::new_with_client_and_backoff(
    "test".to_string(),
    outflow_stats,
    config,
    Arc::new(client),
    Arc::new(|| Box::new(backoff::backoff::Zero {})),
    shutdown_trigger.make_shutdown(),
  )
  .await
  .unwrap();

  (outflow, shutdown_trigger)
}

fn generate_samples(n: usize) -> Vec<ParsedMetric> {
  let parsed_metric = ParsedMetric::try_from_wire_protocol(
    "node_request.count 1 100 source=hello".into(),
    &make_carbon_wire_protocol(),
    Instant::now(),
    DownstreamId::LocalOrigin,
  )
  .unwrap();
  (1 ..= n)
    .map(|i| {
      let mut metric = parsed_metric.metric().clone();
      metric.timestamp = i as u64;
      ParsedMetric::new(
        metric,
        parsed_metric.source().clone(),
        parsed_metric.received_at(),
        DownstreamId::LocalOrigin,
      )
    })
    .collect()
}

fn decode_body(compressed_write_request: &[u8]) -> anyhow::Result<WriteRequest> {
  let decompressed = snap::raw::Decoder::new().decompress_vec(compressed_write_request)?;
  let r = WriteRequest::parse_from_bytes(&decompressed)?;
  Ok(r)
}

fn expect_send_write_request(
  mock_client: &mut MockPromRemoteWriteClient,
  call_count_tx: mpsc::Sender<()>,
  n_expected_samples: usize,
  times: usize,
  result: impl Fn() -> Result<(), PromRemoteWriteError> + Send + 'static,
) {
  mock_client
    .expect_send_write_request()
    .times(times)
    .returning(move |bytes, _| {
      let write_request: WriteRequest = decode_body(&bytes).unwrap();
      let (metrics, errors) = ParsedMetric::from_write_request(
        write_request,
        Instant::now(),
        &ParseConfig::default(),
        &TestDownstreamIdProvider {},
      );
      assert!(errors.is_empty());
      log::debug!("got {} metric(s)", metrics.len());
      assert_eq!(metrics.len(), n_expected_samples);
      call_count_tx.try_send(()).unwrap();
      result()
    });
}

#[allow(clippy::needless_pass_by_ref_mut)] // Spurious
async fn wait_for(call_count_rx: &mut mpsc::Receiver<()>, n_expected_calls: usize) {
  timeout(Duration::from_millis(100), async {
    let mut n_calls = 0;
    while call_count_rx.recv().await == Some(()) {
      n_calls += 1;
      if n_calls >= n_expected_calls {
        return;
      }
    }
  })
  .await
  .unwrap();
}

#[tokio::test]
async fn prom_remote_write_outflow_success() {
  let (call_count_tx, _call_count_rx) = mpsc::channel::<()>(100);
  let mut mock_client = MockPromRemoteWriteClient::new();
  let samples = generate_samples(13);

  // First batch will have 3 metrics (LIFO)
  expect_send_write_request(&mut mock_client, call_count_tx.clone(), 3, 1, || Ok(()));

  // 2nd two batches will have 5 metrics each (LIFO)
  expect_send_write_request(&mut mock_client, call_count_tx.clone(), 5, 2, || Ok(()));

  let (prom_outflow, shutdown_trigger) = setup_test(mock_client).await;

  prom_outflow.recv_samples(samples).await;

  drop(prom_outflow);
  timeout(250.std_milliseconds(), shutdown_trigger.shutdown())
    .await
    .unwrap();
}

#[tokio::test]
async fn prom_remote_write_outflow_retries() {
  let (call_count_tx, mut call_count_rx) = mpsc::channel::<()>(100);
  let mut mock_client = MockPromRemoteWriteClient::new();
  let samples = generate_samples(10);

  // Return a permanent error for the first batch.
  expect_send_write_request(&mut mock_client, call_count_tx.clone(), 5, 1, || {
    Err(PromRemoteWriteError::Response(
      StatusCode::BAD_REQUEST,
      String::new(),
    ))
  });

  // Return a transient error for the second batch twice...
  expect_send_write_request(&mut mock_client, call_count_tx.clone(), 5, 2, || {
    Err(PromRemoteWriteError::Response(
      StatusCode::INTERNAL_SERVER_ERROR,
      String::new(),
    ))
  });

  // And then finally succeed.
  expect_send_write_request(&mut mock_client, call_count_tx.clone(), 5, 1, || Ok(()));

  let (prom_outflow, shutdown_trigger) = setup_test(mock_client).await;

  prom_outflow.recv_samples(samples).await;

  wait_for(&mut call_count_rx, 4).await;
  drop(prom_outflow);
  timeout(250.std_milliseconds(), shutdown_trigger.shutdown())
    .await
    .unwrap();
}

#[tokio::test]
async fn prom_remote_write_outflow_abort_retries() {
  let mut mock_client = MockPromRemoteWriteClient::new();
  let samples = generate_samples(5);

  // Always fail...
  mock_client
    .expect_send_write_request()
    .returning(move |_, _| {
      Err(PromRemoteWriteError::Response(
        StatusCode::INTERNAL_SERVER_ERROR,
        String::new(),
      ))
    });

  let (prom_outflow, shutdown_trigger) = setup_test(mock_client).await;

  prom_outflow.recv_samples(samples).await;

  // And then assert that retries are aborted and the component shuts down.
  drop(prom_outflow);
  timeout(250.std_milliseconds(), shutdown_trigger.shutdown())
    .await
    .unwrap();
}
