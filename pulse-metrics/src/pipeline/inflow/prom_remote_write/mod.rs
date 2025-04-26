// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./mod_test.rs"]
mod mod_test;

use super::http_inflow::{DecodeError, HttpInflow};
use super::{InflowFactoryContext, PipelineInflow};
use crate::pipeline::inflow::http_inflow::make_error_response;
use crate::protos::metric::{DownstreamIdProvider, ParsedMetric};
use async_trait::async_trait;
use axum::response::{IntoResponse, Response};
use bd_log::warn_every;
use bd_proto::protos::prometheus::prompb::remote::WriteRequest;
use bytes::Bytes;
use http::StatusCode;
use itertools::Itertools;
use prom_remote_write::PromRemoteWriteServerConfig;
use prom_remote_write::prom_remote_write_server_config::ParseConfig;
use protobuf::Message;
use pulse_protobuf::protos::pulse::config::inflow::v1::prom_remote_write;
use std::sync::Arc;
use std::time::Instant;
use time::ext::NumericalDuration;

//
// PromRemoteWriteInflow
//

pub(super) struct PromRemoteWriteInflow {
  http_inflow: Arc<HttpInflow>,
}

impl PromRemoteWriteInflow {
  pub async fn new(
    config: PromRemoteWriteServerConfig,
    context: InflowFactoryContext,
  ) -> anyhow::Result<Self> {
    Ok(Self {
      http_inflow: Arc::new(
        HttpInflow::new(
          config.bind.to_string(),
          "/api/v1/prom/write".to_string(),
          config.downstream_id_source.unwrap_or_default(),
          context,
          Box::new(move |inflow, _headers, body, downstream_id_provider| {
            prom_request_to_response(&config.parse_config, inflow, &body, downstream_id_provider)
          }),
        )
        .await?,
      ),
    })
  }
}

#[async_trait]
impl PipelineInflow for PromRemoteWriteInflow {
  async fn start(self: Arc<Self>) {
    self.http_inflow.clone().start();
  }
}

fn prom_request_to_response(
  parse_config: &ParseConfig,
  inflow: &HttpInflow,
  body: &Bytes,
  downstream_id_provider: &dyn DownstreamIdProvider,
) -> (Vec<ParsedMetric>, Response) {
  let received_at = Instant::now();
  let write_request = match decode_body_into_write_request(body) {
    Ok(wr) => wr,
    Err(e) => {
      inflow.stats.requests_4xx.inc();
      warn_every!(
        1.minutes(),
        "prometheus remote write request body failed to decode: {}",
        e
      );
      return (
        vec![],
        make_error_response(
          StatusCode::BAD_REQUEST,
          format!("body failed to decode: {e}"),
        ),
      );
    },
  };

  log::trace!("WriteRequest received: {write_request}");
  let (samples, errors) = ParsedMetric::from_write_request(
    write_request,
    received_at,
    parse_config,
    downstream_id_provider,
  );

  // TODO(mattklein123): Integration test for prom inflow which makes sure we pass through metrics
  // after a failure.
  if errors.is_empty() {
    (samples, StatusCode::OK.into_response())
  } else {
    inflow.stats.requests_4xx.inc();
    let errors = errors.into_iter().map(|e| e.to_string()).join(",");
    warn_every!(1.minutes(), "invalid write request: {}", errors);
    (
      samples,
      make_error_response(
        StatusCode::BAD_REQUEST,
        format!("invalid write request: {errors}"),
      ),
    )
  }
}

fn decode_body_into_write_request(body: &[u8]) -> Result<WriteRequest, DecodeError> {
  let decompressed = snap::raw::Decoder::new().decompress_vec(body)?;
  log::debug!(
    "decompressed WriteRequest from {} bytes to {} bytes",
    body.len(),
    decompressed.len()
  );
  let write_request = WriteRequest::parse_from_tokio_bytes(&Bytes::from(decompressed))?;
  Ok(write_request)
}
