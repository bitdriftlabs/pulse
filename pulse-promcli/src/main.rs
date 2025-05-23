// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use anyhow::bail;
use clap::Parser;
use itertools::Itertools;
use pulse_metrics::clients::http::{
  HttpRemoteWriteClient,
  HyperHttpRemoteWriteClient,
  PROM_REMOTE_WRITE_HEADERS,
};
use pulse_metrics::pipeline::outflow::prom::compress_write_request;
use pulse_metrics::protos::metric::{
  DownstreamId,
  Metric,
  MetricId,
  MetricSource,
  MetricValue,
  ParsedMetric,
  TagValue,
};
use pulse_metrics::protos::prom::{
  ChangedTypeTracker,
  MetadataType,
  ToWriteRequestOptions,
  to_write_request,
};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use time::ext::NumericalDuration;

#[derive(Parser)]
struct Options {
  /// Prometheus remote write endpoint to write to.
  #[arg(long)]
  endpoint: String,

  /// Metric name to write.
  #[arg(long)]
  name: String,

  /// Metric value to write.
  #[arg(long)]
  value: f64,

  /// Metrics tags pairs. Delimited with ':'.
  #[arg(long)]
  tag: Vec<String>,

  /// If set, write request "metadata" will not be emitted.
  #[arg(long, default_value_t = false)]
  no_emit_metadata: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  let options = Options::parse();

  let tags = options
    .tag
    .into_iter()
    .map(|t| {
      let splits: Vec<&str> = t.split(':').collect();
      if splits.len() != 2 {
        bail!("--tag must be in the format of <name>:<value>");
      }
      Ok(TagValue {
        tag: splits[0].to_string().into(),
        value: splits[1].to_string().into(),
      })
    })
    .try_collect()?;

  let metric = ParsedMetric::new(
    Metric::new(
      MetricId::new(options.name.into(), None, tags, false)?,
      None,
      SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs(),
      MetricValue::Simple(options.value),
    ),
    MetricSource::PromRemoteWrite,
    Instant::now(),
    DownstreamId::LocalOrigin,
  );

  let write_request = to_write_request(
    vec![metric],
    &ToWriteRequestOptions {
      metadata: if options.no_emit_metadata {
        MetadataType::None
      } else {
        MetadataType::Normal
      },
      convert_name: true,
    },
    &ChangedTypeTracker::new_for_test(),
  );
  let client = HyperHttpRemoteWriteClient::new(
    options.endpoint,
    10.seconds(),
    None,
    PROM_REMOTE_WRITE_HEADERS,
    vec![],
  )
  .await?;
  client
    .send_write_request(compress_write_request(&write_request).into(), None)
    .await?;

  Ok(())
}
