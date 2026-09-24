// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/licenses/strict/1.0.0.txt

use super::http::remote_write::{BatchRouter, DefaultBatchRouter, HttpRemoteWriteOutflow};
use super::{OutflowFactoryContext, OutflowStats};
use crate::protos::metric::{CounterType, MetricType, ParsedMetric};
use crate::protos::prom::prom_name;
use bd_otlp_metrics::protos::common::any_value::Value;
use bd_otlp_metrics::protos::common::{AnyValue, KeyValue};
use bd_otlp_metrics::protos::metrics::metric::Data;
use bd_otlp_metrics::protos::metrics::summary_data_point::ValueAtQuantile;
use bd_otlp_metrics::protos::metrics::{
  AggregationTemporality,
  Gauge,
  Histogram,
  HistogramDataPoint,
  Metric,
  NumberDataPoint,
  Sum,
  Summary,
  SummaryDataPoint,
  number_data_point,
};
use bd_otlp_metrics::{
  MetricId as SharedMetricId,
  OtlpCompression as SharedOtlpCompression,
  TagValue as SharedTagValue,
  deserialize_otlp_metrics_request,
  encode_otlp_metrics,
};
use bd_shutdown::ComponentShutdown;
use bytes::Bytes;
use http::HeaderMap;
use http::header::CONTENT_ENCODING;
use protobuf::Chars;
use pulse_common::LossyFloatToInt;
use pulse_protobuf::protos::pulse::config::outflow::v1::otlp::OtlpClientConfig;
use pulse_protobuf::protos::pulse::config::outflow::v1::otlp::otlp_client_config::OtlpCompression;
use std::sync::Arc;

pub fn make_otlp_batch_router(
  config: &OtlpClientConfig,
  stats: &OutflowStats,
  shutdown: ComponentShutdown,
) -> Arc<dyn BatchRouter> {
  let compression = config.compression.enum_value_or_default();
  let convert_names_to_prom = config.convert_names_to_prometheus;
  let finisher =
    Arc::new(move |samples| finish_otlp_batch(samples, compression, convert_names_to_prom));

  Arc::new(DefaultBatchRouter::new(
    config.batch_max_samples,
    None,
    &config.queue_policy,
    &stats.stats,
    shutdown,
    match compression {
      OtlpCompression::NONE => None,
      OtlpCompression::SNAPPY => {
        let mut header_map = HeaderMap::new();
        header_map.insert(CONTENT_ENCODING, "snappy".parse().unwrap());
        Some(Arc::new(header_map))
      },
    },
    finisher,
  ))
}

#[allow(clippy::large_futures)]
pub async fn make_otlp_outflow(
  config: OtlpClientConfig,
  context: OutflowFactoryContext,
) -> anyhow::Result<Arc<HttpRemoteWriteOutflow>> {
  let batch_router = make_otlp_batch_router(
    &config,
    &context.stats,
    context.shutdown_trigger_handle.make_shutdown(),
  );
  let compression = config.compression.enum_value_or_default();
  HttpRemoteWriteOutflow::new(
    config.request_timeout,
    config.connect_timeout,
    config.retry_policy.unwrap_or_default(),
    config.max_in_flight,
    batch_router,
    config.send_to.to_string(),
    config.auth.into_option(),
    &[],
    config.request_headers,
    context,
    None.into(),
    Arc::new(move |bytes| deserialize_otlp_request(bytes, compression)),
  )
  .await
}

#[allow(dead_code)]
fn tags_to_key_value(metric: &ParsedMetric, convert_names_to_prom: bool) -> Vec<KeyValue> {
  metric
    .metric()
    .get_id()
    .tags()
    .iter()
    .filter_map(|tag| {
      Some(KeyValue {
        key: Chars::from_bytes(prom_name(
          &tag.tag,
          b'_',
          convert_names_to_prom,
          metric.source(),
        ))
        .ok()?,
        value: Some(AnyValue {
          value: Some(Value::StringValue(
            Chars::from_bytes(tag.value.clone()).ok()?,
          )),
          ..Default::default()
        })
        .into(),
        ..Default::default()
      })
    })
    .collect()
}

#[allow(dead_code)]
fn make_simple_metric(
  samples: Vec<ParsedMetric>,
  name: Bytes,
  mtype: MetricType,
  convert_names_to_prom: bool,
) -> Option<Metric> {
  let data_points = samples
    .into_iter()
    .map(|sample| NumberDataPoint {
      attributes: tags_to_key_value(&sample, convert_names_to_prom),
      time_unix_nano: sample.metric().timestamp * 1_000_000_000,
      value: Some(number_data_point::Value::AsDouble(
        sample.metric().value.to_simple(),
      )),
      ..Default::default()
    })
    .collect();

  Some(Metric {
    name: Chars::from_bytes(name).ok()?,
    data: Some(match mtype {
      MetricType::Gauge | MetricType::DirectGauge => Data::Gauge(Gauge {
        data_points,
        ..Default::default()
      }),
      MetricType::Counter(counter_type) => Data::Sum(Sum {
        data_points,
        aggregation_temporality: match counter_type {
          CounterType::Absolute => {
            AggregationTemporality::AGGREGATION_TEMPORALITY_CUMULATIVE.into()
          },
          CounterType::Delta => AggregationTemporality::AGGREGATION_TEMPORALITY_DELTA.into(),
        },
        ..Default::default()
      }),
      _ => unreachable!(),
    }),
    ..Default::default()
  })
}

#[allow(dead_code)]
fn make_histogram_metric(
  samples: Vec<ParsedMetric>,
  name: Bytes,
  convert_names_to_prom: bool,
) -> Option<Metric> {
  let data_points = samples
    .into_iter()
    .map(|sample| {
      let histogram = sample.metric().value.to_histogram();
      let mut bucket_counts: Vec<u64> = histogram
        .buckets
        .iter()
        .enumerate()
        .map(|(i, bucket)| {
          if i == 0 {
            bucket.count
          } else {
            bucket.count - histogram.buckets[i - 1].count
          }
          .lossy_to_u64()
        })
        .collect();
      if !bucket_counts.is_empty() {
        bucket_counts
          .push((histogram.sample_count - histogram.buckets.last().unwrap().count).lossy_to_u64());
      }

      HistogramDataPoint {
        attributes: tags_to_key_value(&sample, convert_names_to_prom),
        time_unix_nano: sample.metric().timestamp * 1_000_000_000,
        count: histogram.sample_count.lossy_to_u64(),
        sum: Some(histogram.sample_sum),
        bucket_counts,
        explicit_bounds: histogram.buckets.iter().map(|bucket| bucket.le).collect(),
        ..Default::default()
      }
    })
    .collect();

  Some(Metric {
    name: Chars::from_bytes(name).ok()?,
    data: Some(Data::Histogram(Histogram {
      data_points,
      aggregation_temporality: AggregationTemporality::AGGREGATION_TEMPORALITY_CUMULATIVE.into(),
      ..Default::default()
    })),
    ..Default::default()
  })
}

#[allow(dead_code)]
fn make_summary_metric(
  samples: Vec<ParsedMetric>,
  name: Bytes,
  convert_names_to_prom: bool,
) -> Option<Metric> {
  let data_points = samples
    .into_iter()
    .map(|sample| {
      let summary = sample.metric().value.to_summary();
      let quantile_values = summary
        .quantiles
        .iter()
        .map(|quantile| ValueAtQuantile {
          quantile: quantile.quantile,
          value: quantile.value,
          ..Default::default()
        })
        .collect();

      SummaryDataPoint {
        attributes: tags_to_key_value(&sample, convert_names_to_prom),
        time_unix_nano: sample.metric().timestamp * 1_000_000_000,
        count: summary.sample_count.lossy_to_u64(),
        sum: summary.sample_sum,
        quantile_values,
        ..Default::default()
      }
    })
    .collect();

  Some(Metric {
    name: Chars::from_bytes(name).ok()?,
    data: Some(Data::Summary(Summary {
      data_points,
      ..Default::default()
    })),
    ..Default::default()
  })
}

#[must_use]
pub fn finish_otlp_batch(
  samples: Vec<ParsedMetric>,
  compression: OtlpCompression,
  convert_names_to_prom: bool,
) -> Bytes {
  let metrics = samples
    .into_iter()
    .map(|sample| {
      let mut metric = sample.metric().clone();
      let (name, metric_type, tags) = metric.get_id().clone().into_parts();
      let tags = tags
        .into_iter()
        .map(|tag| SharedTagValue {
          tag: prom_name(&tag.tag, b'_', convert_names_to_prom, sample.source()),
          value: tag.value,
        })
        .collect();
      metric.set_id(
        SharedMetricId::new(
          prom_name(&name, b':', convert_names_to_prom, sample.source()),
          metric_type,
          tags,
          true,
        )
        .expect("Pulse metric IDs have already been length validated"),
      );
      metric
    })
    .collect();
  encode_otlp_metrics(
    metrics,
    match compression {
      OtlpCompression::NONE => SharedOtlpCompression::None,
      OtlpCompression::SNAPPY => SharedOtlpCompression::Snappy,
    },
  )
}

fn deserialize_otlp_request(compressed_bytes: &[u8], compression: OtlpCompression) -> String {
  deserialize_otlp_metrics_request(
    compressed_bytes,
    match compression {
      OtlpCompression::NONE => SharedOtlpCompression::None,
      OtlpCompression::SNAPPY => SharedOtlpCompression::Snappy,
    },
  )
}
