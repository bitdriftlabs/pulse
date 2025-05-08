// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::http::remote_write::{BatchRouter, DefaultBatchRouter, HttpRemoteWriteOutflow};
use super::{OutflowFactoryContext, OutflowStats};
use crate::protos::metric::{CounterType, MetricType, ParsedMetric, TagValue};
use bd_log::warn_every;
use bd_shutdown::ComponentShutdown;
use bytes::Bytes;
use hashbrown::HashMap;
use http::HeaderMap;
use http::header::CONTENT_ENCODING;
use protobuf::{Chars, Message};
use pulse_common::LossyFloatToInt;
use pulse_protobuf::protos::opentelemetry::common::any_value::Value;
use pulse_protobuf::protos::opentelemetry::common::{AnyValue, KeyValue};
use pulse_protobuf::protos::opentelemetry::metrics::metric::Data;
use pulse_protobuf::protos::opentelemetry::metrics::summary_data_point::ValueAtQuantile;
use pulse_protobuf::protos::opentelemetry::metrics::{
  AggregationTemporality,
  Gauge,
  Histogram,
  HistogramDataPoint,
  Metric,
  NumberDataPoint,
  ResourceMetrics,
  ScopeMetrics,
  Sum,
  Summary,
  SummaryDataPoint,
  number_data_point,
};
use pulse_protobuf::protos::opentelemetry::metrics_service::ExportMetricsServiceRequest;
use pulse_protobuf::protos::pulse::config::outflow::v1::otlp::OtlpClientConfig;
use pulse_protobuf::protos::pulse::config::outflow::v1::otlp::otlp_client_config::OtlpCompression;
use std::io::Write;
use std::sync::Arc;
use time::ext::NumericalDuration;

pub fn make_otlp_batch_router(
  config: &OtlpClientConfig,
  stats: &OutflowStats,
  shutdown: ComponentShutdown,
) -> Arc<dyn BatchRouter> {
  let compression = config.compression.enum_value_or_default();
  let finisher = Arc::new(move |samples| finish_otlp_batch(samples, compression));

  Arc::new(DefaultBatchRouter::new(
    config.batch_max_samples,
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

pub async fn make_otlp_outflow(
  config: OtlpClientConfig,
  context: OutflowFactoryContext,
) -> anyhow::Result<Arc<HttpRemoteWriteOutflow>> {
  let batch_router = make_otlp_batch_router(
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
    &[],
    config.request_headers,
    context,
  )
  .await
}

fn tags_to_key_value(tags: &[TagValue]) -> Vec<KeyValue> {
  tags
    .iter()
    .filter_map(|tag| {
      Some(KeyValue {
        key: Chars::from_bytes(tag.tag.clone()).ok()?,
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

fn make_simple_metric(
  samples: Vec<ParsedMetric>,
  name: Bytes,
  mtype: MetricType,
) -> Option<Metric> {
  let data_points = samples
    .into_iter()
    .map(|sample| NumberDataPoint {
      attributes: tags_to_key_value(sample.metric().get_id().tags()),
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

fn make_histogram_metric(samples: Vec<ParsedMetric>, name: Bytes) -> Option<Metric> {
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
        attributes: tags_to_key_value(sample.metric().get_id().tags()),
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

fn make_summary_metric(samples: Vec<ParsedMetric>, name: Bytes) -> Option<Metric> {
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
        attributes: tags_to_key_value(sample.metric().get_id().tags()),
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
pub fn finish_otlp_batch(samples: Vec<ParsedMetric>, compression: OtlpCompression) -> Bytes {
  let metrics_by_name_and_type: HashMap<(Bytes, MetricType), Vec<ParsedMetric>> =
    samples.into_iter().fold(HashMap::new(), |mut acc, sample| {
      let key = (
        sample.metric().get_id().name().clone(),
        sample
          .metric()
          .get_id()
          .mtype()
          .unwrap_or(MetricType::Gauge),
      );
      acc.entry(key).or_default().push(sample);
      acc
    });

  let mut metrics = Vec::new();
  for ((name, mtype), samples) in metrics_by_name_and_type {
    let metric = match mtype {
      MetricType::Gauge | MetricType::DirectGauge | MetricType::Counter(_) => {
        make_simple_metric(samples, name, mtype)
      },
      MetricType::Histogram => make_histogram_metric(samples, name),
      MetricType::Summary => make_summary_metric(samples, name),
      MetricType::DeltaGauge | MetricType::Timer | MetricType::BulkTimer => {
        warn_every!(1.minutes(), "unsupported OTLP metric type: {:?}", mtype);
        None
      },
    };
    if let Some(metric) = metric {
      metrics.push(metric);
    }
  }

  let request = ExportMetricsServiceRequest {
    resource_metrics: vec![ResourceMetrics {
      scope_metrics: vec![ScopeMetrics {
        metrics,
        ..Default::default()
      }],
      ..Default::default()
    }],
    ..Default::default()
  };

  log::trace!("ExportMetricsServiceRequest batched and ready to send: {request}");
  let uncompressed_write_request = request.write_to_bytes().unwrap();
  let compressed_write_request = match compression {
    OtlpCompression::NONE => uncompressed_write_request,
    OtlpCompression::SNAPPY => {
      log::trace!("compressing ExportMetricsServiceRequest with snappy");
      let mut compressed = Vec::new();
      snap::write::FrameEncoder::new(&mut compressed)
        .write_all(&uncompressed_write_request)
        .unwrap();
      compressed
    },
  };

  compressed_write_request.into()
}
