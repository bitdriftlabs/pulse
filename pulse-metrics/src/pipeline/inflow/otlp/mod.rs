// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::http_inflow::HttpInflow;
use super::{InflowFactoryContext, PipelineInflow};
use crate::protos::metric::{
  CounterType,
  DownstreamIdProvider,
  HistogramBucket,
  HistogramData,
  Metric,
  MetricId,
  MetricSource,
  MetricType,
  MetricValue,
  ParsedMetric,
  SummaryBucket,
  SummaryData,
  TagValue,
};
use anyhow::bail;
use async_trait::async_trait;
use axum::response::Response;
use bd_log::warn_every;
use bytes::Bytes;
use hashbrown::HashMap;
use http::{HeaderMap, StatusCode};
use protobuf::Message;
use pulse_common::LossyIntToFloat;
use pulse_protobuf::protos::opentelemetry::common::KeyValue;
use pulse_protobuf::protos::opentelemetry::common::any_value::Value as AnyValue;
use pulse_protobuf::protos::opentelemetry::metrics::metric::Data;
use pulse_protobuf::protos::opentelemetry::metrics::number_data_point::Value;
use pulse_protobuf::protos::opentelemetry::metrics::{
  AggregationTemporality,
  HistogramDataPoint,
  Metric as OtlpMetric,
  NumberDataPoint,
  SummaryDataPoint,
};
use pulse_protobuf::protos::opentelemetry::metrics_service::{
  ExportMetricsServiceRequest,
  ExportMetricsServiceResponse,
};
use pulse_protobuf::protos::pulse::config::inflow::v1::otlp::OtlpServerConfig;
use std::io::Read;
use std::sync::Arc;
use std::time::Instant;
use time::ext::NumericalDuration;

//
// OtlpInflow
//

pub(super) struct OtlpInflow {
  http_inflow: Arc<HttpInflow>,
}

impl OtlpInflow {
  pub async fn new(
    config: OtlpServerConfig,
    context: InflowFactoryContext,
  ) -> anyhow::Result<Self> {
    let parse_config = ParseConfig {
      include_resource_attributes: config.include_resource_attributes.unwrap_or(true),
      include_scope_attributes: config.include_scope_attributes.unwrap_or(true),
    };

    Ok(Self {
      http_inflow: Arc::new(
        HttpInflow::new(
          config.bind.to_string(),
          config
            .path
            .map_or("/v1/metrics".to_string(), |p| p.to_string()),
          config.downstream_id_source.unwrap_or_default(),
          context,
          Box::new(move |_inflow, headers, body, downstream_id_provider| {
            otlp_request_to_response(&headers, body, downstream_id_provider, &parse_config)
          }),
        )
        .await?,
      ),
    })
  }
}

#[async_trait]
impl PipelineInflow for OtlpInflow {
  async fn start(self: Arc<Self>) {
    self.http_inflow.clone().start();
  }
}

fn kv_to_iterator(metadata: Vec<KeyValue>) -> impl Iterator<Item = (Bytes, Bytes)> {
  metadata.into_iter().filter_map(|kv| {
    let value = match kv.value.into_option().and_then(|v| v.value) {
      None => None,
      Some(AnyValue::StringValue(v)) => Some(v),
      e => {
        warn_every!(1.minutes(), "unsupported OTLP KeyValue type: {:?}", e);
        None
      },
    };

    value.map(|v| (kv.key.into_bytes(), v.into_bytes()))
  })
}

fn kv_to_map(metadata: Vec<KeyValue>) -> HashMap<Bytes, Bytes> {
  kv_to_iterator(metadata).collect()
}

fn final_kv_merge(
  resource_attributes: Option<&HashMap<Bytes, Bytes>>,
  scope_attributes: Option<&HashMap<Bytes, Bytes>>,
  kv: Vec<KeyValue>,
) -> Vec<TagValue> {
  // Optimize the case where no merging is needed.
  if resource_attributes.is_none() && scope_attributes.is_none() {
    return kv_to_iterator(kv)
      .map(|(tag, value)| TagValue { tag, value })
      .collect();
  }

  let mut attributes = HashMap::new();
  if let Some(resource_attributes) = resource_attributes {
    for (tag, value) in resource_attributes {
      attributes.insert(tag.clone(), value.clone());
    }
  }
  if let Some(scope_attributes) = scope_attributes {
    for (tag, value) in scope_attributes {
      attributes.insert(tag.clone(), value.clone());
    }
  }
  attributes.extend(kv_to_iterator(kv));
  attributes
    .into_iter()
    .map(|(tag, value)| TagValue { tag, value })
    .collect()
}

fn number_data_point_to_metric(
  name: Bytes,
  mtype: MetricType,
  dp: NumberDataPoint,
  received_at: Instant,
  downstream_id_provider: &dyn DownstreamIdProvider,
  resource_attributes: Option<&HashMap<Bytes, Bytes>>,
  scope_attributes: Option<&HashMap<Bytes, Bytes>>,
) -> Option<ParsedMetric> {
  let attributes = final_kv_merge(resource_attributes, scope_attributes, dp.attributes);
  let timestamp = dp.time_unix_nano / 1_000_000_000;
  let metric_id = MetricId::new(name, Some(mtype), attributes, false).ok()?;
  let v = match dp.value? {
    Value::AsInt(i) => i.lossy_to_f64(),
    Value::AsDouble(d) => d,
  };

  let downstream_id = downstream_id_provider.downstream_id(&metric_id);
  let metric = Metric::new(metric_id, None, timestamp, MetricValue::Simple(v));
  Some(ParsedMetric::new(
    metric,
    MetricSource::Otlp,
    received_at,
    downstream_id,
  ))
}

fn histogram_data_point_to_metric(
  name: Bytes,
  dp: HistogramDataPoint,
  received_at: Instant,
  downstream_id_provider: &dyn DownstreamIdProvider,
  resource_attributes: Option<&HashMap<Bytes, Bytes>>,
  scope_attributes: Option<&HashMap<Bytes, Bytes>>,
) -> anyhow::Result<ParsedMetric> {
  match (dp.bucket_counts.is_empty(), dp.explicit_bounds.is_empty()) {
    (true, true) | (false, false) => {},
    _ => {
      bail!("OTLP histogram data point must have both bucket_counts and explicit_bounds or neither")
    },
  }

  if !dp.bucket_counts.is_empty() && dp.bucket_counts.len() != dp.explicit_bounds.len() + 1 {
    bail!(
      "OTLP histogram data point bucket_counts must have one more element than explicit_bounds"
    );
  }

  let attributes = final_kv_merge(resource_attributes, scope_attributes, dp.attributes);
  let timestamp = dp.time_unix_nano / 1_000_000_000;
  let metric_id = MetricId::new(name, Some(MetricType::Histogram), attributes, false)?;

  let mut buckets = Vec::new();
  for (count, le) in dp.bucket_counts[0 .. dp.bucket_counts.len() - 1]
    .iter()
    .zip(dp.explicit_bounds.iter())
  {
    if buckets.is_empty() {
      buckets.push(HistogramBucket {
        le: *le,
        count: count.lossy_to_f64(),
      });
    } else {
      buckets.push(HistogramBucket {
        le: *le,
        count: (count.lossy_to_f64() + buckets.last().unwrap().count),
      });
    }
  }
  let sample_count =
    dp.bucket_counts.last().unwrap().lossy_to_f64() + buckets.last().unwrap().count;

  let downstream_id = downstream_id_provider.downstream_id(&metric_id);
  let metric = Metric::new(
    metric_id,
    None,
    timestamp,
    MetricValue::Histogram(HistogramData {
      buckets,
      sample_sum: dp.sum.unwrap_or_default(),
      sample_count,
    }),
  );
  Ok(ParsedMetric::new(
    metric,
    MetricSource::Otlp,
    received_at,
    downstream_id,
  ))
}

fn summary_data_point_to_metric(
  name: Bytes,
  dp: SummaryDataPoint,
  received_at: Instant,
  downstream_id_provider: &dyn DownstreamIdProvider,
  resource_attributes: Option<&HashMap<Bytes, Bytes>>,
  scope_attributes: Option<&HashMap<Bytes, Bytes>>,
) -> Option<ParsedMetric> {
  let attributes = final_kv_merge(resource_attributes, scope_attributes, dp.attributes);
  let timestamp = dp.time_unix_nano / 1_000_000_000;
  let metric_id = MetricId::new(name, Some(MetricType::Summary), attributes, false).ok()?;
  let downstream_id = downstream_id_provider.downstream_id(&metric_id);
  let metric = Metric::new(
    metric_id,
    None,
    timestamp,
    MetricValue::Summary(SummaryData {
      quantiles: dp
        .quantile_values
        .into_iter()
        .map(|qv| SummaryBucket {
          quantile: qv.quantile,
          value: qv.value,
        })
        .collect(),
      sample_sum: dp.sum,
      sample_count: dp.count.lossy_to_f64(),
    }),
  );
  Some(ParsedMetric::new(
    metric,
    MetricSource::Otlp,
    received_at,
    downstream_id,
  ))
}

fn metric_to_samples(
  metric: OtlpMetric,
  samples: &mut Vec<ParsedMetric>,
  received_at: Instant,
  downstream_id_provider: &dyn DownstreamIdProvider,
  resource_attributes: Option<&HashMap<Bytes, Bytes>>,
  scope_attributes: Option<&HashMap<Bytes, Bytes>>,
) -> Option<()> {
  match metric.data? {
    Data::Gauge(gauge) => {
      for dp in gauge.data_points {
        if let Some(sample) = number_data_point_to_metric(
          metric.name.clone().into_bytes(),
          MetricType::Gauge,
          dp,
          received_at,
          downstream_id_provider,
          resource_attributes,
          scope_attributes,
        ) {
          samples.push(sample);
        }
      }
    },
    Data::Sum(sum) => {
      for dp in sum.data_points {
        if let Some(sample) = number_data_point_to_metric(
          metric.name.clone().into_bytes(),
          MetricType::Counter(match sum.aggregation_temporality.enum_value_or_default() {
            AggregationTemporality::AGGREGATION_TEMPORALITY_CUMULATIVE => CounterType::Absolute,
            AggregationTemporality::AGGREGATION_TEMPORALITY_DELTA => CounterType::Delta,
            AggregationTemporality::AGGREGATION_TEMPORALITY_UNSPECIFIED => {
              warn_every!(1.minutes(), "{}", "unspecified aggregation temporality");
              return None;
            },
          }),
          dp,
          received_at,
          downstream_id_provider,
          resource_attributes,
          scope_attributes,
        ) {
          samples.push(sample);
        }
      }
    },
    Data::Histogram(histogram) => {
      if histogram.aggregation_temporality
        != AggregationTemporality::AGGREGATION_TEMPORALITY_CUMULATIVE.into()
      {
        warn_every!(
          1.minutes(),
          "unsupported histogram aggregation temporality: {:?}",
          histogram.aggregation_temporality
        );
        return None;
      }
      for dp in histogram.data_points {
        match histogram_data_point_to_metric(
          metric.name.clone().into_bytes(),
          dp,
          received_at,
          downstream_id_provider,
          resource_attributes,
          scope_attributes,
        ) {
          Ok(sample) => samples.push(sample),
          Err(e) => {
            warn_every!(1.minutes(), "failed to parse histogram: {}", e);
            return None;
          },
        }
      }
    },
    Data::ExponentialHistogram(_exponential_histogram) => {
      warn_every!(
        1.minutes(),
        "{}",
        "OTLP exponential histogram not supported yet"
      );
    },
    Data::Summary(summary) => {
      for dp in summary.data_points {
        if let Some(sample) = summary_data_point_to_metric(
          metric.name.clone().into_bytes(),
          dp,
          received_at,
          downstream_id_provider,
          resource_attributes,
          scope_attributes,
        ) {
          samples.push(sample);
        }
      }
    },
  }

  Some(())
}

pub struct ParseConfig {
  pub include_resource_attributes: bool,
  pub include_scope_attributes: bool,
}

pub fn metrics_request_to_samples(
  metrics_request: ExportMetricsServiceRequest,
  received_at: Instant,
  downstream_id_provider: &dyn DownstreamIdProvider,
  parse_config: &ParseConfig,
) -> Vec<ParsedMetric> {
  let mut samples = Vec::new();

  for resource_metrics in metrics_request.resource_metrics {
    let resource_attributes = if parse_config.include_resource_attributes {
      Some(kv_to_map(
        resource_metrics.resource.unwrap_or_default().attributes,
      ))
    } else {
      None
    };

    for scope_metrics in resource_metrics.scope_metrics {
      let scope_attributes = if parse_config.include_scope_attributes {
        Some(kv_to_map(
          scope_metrics.scope.unwrap_or_default().attributes,
        ))
      } else {
        None
      };

      for metric in scope_metrics.metrics {
        metric_to_samples(
          metric,
          &mut samples,
          received_at,
          downstream_id_provider,
          resource_attributes.as_ref(),
          scope_attributes.as_ref(),
        );
      }
    }
  }

  samples
}

pub fn decode_otlp_request(
  headers: &HeaderMap,
  body: Bytes,
) -> anyhow::Result<ExportMetricsServiceRequest> {
  let content_encoding = headers
    .get("Content-Encoding")
    .and_then(|v| v.to_str().ok())
    .unwrap_or_default();
  let decompressed_bytes = match content_encoding {
    "" => {
      log::trace!("no decompression");
      body
    },
    "snappy" => {
      log::trace!("snappy decompression");
      let mut decompressed = Vec::new();
      snap::read::FrameDecoder::new(body.as_ref()).read_to_end(&mut decompressed)?;
      decompressed.into()
    },
    _ => bail!("unsupported Content-Encoding: {content_encoding}"),
  };
  Ok(ExportMetricsServiceRequest::parse_from_tokio_bytes(
    &decompressed_bytes,
  )?)
}

fn otlp_request_to_response(
  headers: &HeaderMap,
  body: Bytes,
  downstream_id_provider: &dyn DownstreamIdProvider,
  parse_config: &ParseConfig,
) -> (Vec<ParsedMetric>, Response) {
  let received_at = Instant::now();
  let metrics_request = match decode_otlp_request(headers, body) {
    Ok(request) => request,
    Err(e) => {
      return (
        Vec::new(),
        Response::builder()
          .status(StatusCode::BAD_REQUEST)
          .body(e.to_string().into())
          .unwrap(),
      );
    },
  };
  log::trace!("ExportMetricsServiceRequest received: {metrics_request}");
  let samples = metrics_request_to_samples(
    metrics_request,
    received_at,
    downstream_id_provider,
    parse_config,
  );

  (
    samples,
    Response::builder()
      .status(StatusCode::OK)
      .header("Content-Type", "application/x-protobuf")
      .body(
        ExportMetricsServiceResponse::new()
          .write_to_bytes()
          .unwrap()
          .into(),
      )
      .unwrap(),
  )
}
