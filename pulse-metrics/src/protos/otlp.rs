// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt



use crate::protos::metric::{DownstreamId, DownstreamIdProvider, MetricId, ParsedMetric};

use opentelemetry::sdk::metrics::data::{DataPoint, MetricData};
use std::collections::HashMap;
use std::fmt::Display;
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use protobuf::{Chars, EnumOrUnknown};
use std::collections::BTreeMap;
use pulse_protobuf::protos::pulse::otlp::metrics::v1::metrics::Metric as OtlpMetric;
use pulse_protobuf::protos::pulse::config;
use config::inflow::v1::otlp::otlp_server_config::ParseConfig;


use super::metric::{
    unwrap_timestamp,
    CounterType,
    HistogramBucket,
    HistogramData,
    Metric,
    MetricId,
    MetricSource,
    MetricType,
    MetricValue,
    ParseError,
    ParsedMetric,
    SummaryBucket,
    SummaryData,
    TagValue,
  };


  
// Processing OpenTelemetry Metrics
// MetricsData
// └─── ResourceMetrics
//   ├── Resource
//   ├── SchemaURL
//   └── ScopeMetrics
//      ├── Scope
//      ├── SchemaURL
//      └── Metric
//         ├── Name
//         ├── Description
//         ├── Unit
//         └── data
//            ├── Gauge
//            ├── Sum
//            ├── Histogram
//            ├── ExponentialHistogram
//            └── Summary

// OpenTelemetry Metric Types
// ├── Gauge
// ├── Sum
// ├── Histogram
// ├── ExponentialHistogram
// └── Summary

// OpenTelemetry Metric Data Type Sum
// ├── DataPoints
// │   ├── Attributes
// │   ├── StartTimeUnixNano
// │   ├── TimeUnixNano
// │   ├── Value
// │   ├── AggregationTemporality
// │   ├── IsMonotonic
// │   └── Flags

// OpenTelemetry Metric Data Type Gauge
// ├── DataPoints
// │   ├── Attributes
// │   ├── StartTimeUnixNano
// │   ├── TimeUnixNano
// │   ├── Value
// │   └── Flags

// OpenTelemetry Metric Data Type Histogram
// ├── DataPoints
// │   ├── Attributes
// │   ├── StartTimeUnixNano
// │   ├── TimeUnixNano
// │   ├── Count
// │   ├── Sum
// │   ├── BucketCounts
// │   ├── ExplicitBounds
// │   └── Flags


// OpenTelemetry Metric Data Type ExponentialHistogram
// ├── DataPoints
// │   ├── Attributes
// │   ├── StartTimeUnixNano
// │   ├── TimeUnixNano
// │   ├── Count
// │   ├── Sum
// │   ├── Scale
// │   ├── ZeroCount
// │   ├── Positive
// │   ├── Negative
// │   └── Flags


// OpenTelemetry Metric Data Type Summary
// ├── DataPoints
// │   ├── Attributes
// │   ├── StartTimeUnixNano
// │   ├── TimeUnixNano
// │   ├── Count
// │   ├── Sum
// │   ├── QuantileValues
// │   └── Flags

// For incoming summary data being parsed, whether it refers to quantile info, total count, or total
// sum.
#[derive(Clone, Copy)]
enum SummaryDataType {
  Quantile,
  Count,
  Sum,
}

//
// InProgressSummaryData
//

// A summary that is currently being built from incoming data.
#[derive(Default)]
struct InProgressSummaryData {
  quantiles: Vec<SummaryBucket>,
  sample_count: Option<f64>,
  sample_sum: Option<f64>,
}


//
// InProgressSummary
//

// Tracks a complete summary being parsed, including common tags, as well as data for every
// incoming timestamp.
#[derive(Default)]
struct InProgressSummary {
  summaries: HashMap<
    Vec<TagValue>,
    // BTreeMap is used here so that timestamps are sorted, regardless of input order.
    // TODO(mattklein123): Technically the "spec" says they should be in timestamp order so we
    // could probably just fail if out of order but we do this for now.
    BTreeMap<i64, InProgressSummaryData>,
  >,
}


// Given the tags and samples for an incoming timeseries that is part of a summary, collect the
// samples into the in progress state.
fn process_in_progress_summary(
    summary: &mut InProgressSummary,
    data_type: SummaryDataType,
    mut tags: Vec<TagValue>,
    samples: Vec<Sample>,
  ) -> Result<(), ParseError> {
    let maybe_quantile = if matches!(data_type, SummaryDataType::Quantile) {
      let (quantile_tag_index, quantile_tag_value) = tags
        .iter()
        .find_position(|t| t.tag.as_ref() == b"quantile")
        .unwrap();
      let quantile: f64 = std::str::from_utf8(&quantile_tag_value.value)
        .map_err(|_| ParseError::PromRemoteWrite("invalid quantile tag"))?
        .parse::<f64>()
        .map_err(|_| ParseError::PromRemoteWrite("invalid quantile tag"))?;
      if !quantile.is_finite() {
        return Err(ParseError::PromRemoteWrite("invalid quantile tag"));
      }
      tags.swap_remove(quantile_tag_index);
      Some(quantile)
    } else {
      None
    };
  
    // At this point, the "quantile" tag has been removed if applicable. Now sort the remaining tags
    // so we have a stable hash key.
    tags.sort_unstable();
    let timeseries = summary.summaries.entry(tags).or_default();
  
    // Push the samples into the relevant entry for the given timestamp.
    for sample in samples {
      let data = timeseries.entry(sample.timestamp).or_default();
      match data_type {
        SummaryDataType::Quantile => data.quantiles.push(SummaryBucket {
          quantile: maybe_quantile.unwrap(),
          value: sample.value,
        }),
        SummaryDataType::Count => {
          if data.sample_count.is_some() {
            return Err(ParseError::PromRemoteWrite("duplicate summary count"));
          }
          data.sample_count = Some(sample.value);
        },
        SummaryDataType::Sum => {
          if data.sample_sum.is_some() {
            return Err(ParseError::PromRemoteWrite("duplicate summary sum"));
          }
          data.sample_sum = Some(sample.value);
        },
      }
    }
  
    Ok(())
  }



#[must_use]
pub fn make_label(name: Chars, value: Chars) -> Label {
    Label {
      name,
      value,
      ..Default::default()
    }
  }

// Define OpenTelemetry Metric Types
#[derive(Clone, Debug, PartialEq)]
pub enum OtlpMetricType {
    Counter,
    Gauge,
    Histogram,
    Summary,
}

// Define a structure for OpenTelemetry Metric
#[derive(Clone, Debug)]
pub struct OtlpMetric {
    name: String,
    metric_type: OtlpMetricType,
    labels: HashMap<String, String>,
    data: MetricData,
    timestamp: u64,
}

// covert from otlp metric type to pulse internal metric type



// Convert from internal metric type to otlp metric type.



impl OtlpMetric {
    pub fn new(
        name: String,
        metric_type: OtlpMetricType,
        labels: HashMap<String, String>,
        data: MetricData,
        timestamp: u64,
    ) -> Self {
        Self {
            name,
            metric_type,
            labels,
            data,
            timestamp,
        }
    }

    pub fn to_otlp_format(&self) -> String {
        // Convert the metric to a string representation in OpenTelemetry format
        format!(
            "Metric: {}, Type: {:?}, Labels: {:?}, Data: {:?}, Timestamp: {}",
            self.name, self.metric_type, self.labels, self.data, self.timestamp
        )
    }
}

// Implement Display for OtlpMetric
impl Display for OtlpMetric {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_otlp_format())
    }
}

// Define a structure for handling errors
#[derive(Error, Debug, PartialEq)]
pub enum OtlpParseError {
    #[error("invalid metric type")]
    InvalidMetricType,
    #[error("invalid data point")]
    InvalidDataPoint,
    #[error("name or label length too large")]
    TooLarge,
}

// Define a function to normalize timestamps
pub fn normalize_timestamp(t: u64) -> u64 {
    const MAX_SECONDS_TIMESTAMP: u64 = 100_000_000_000;
    if t > MAX_SECONDS_TIMESTAMP {
        t / 1000
    } else {
        t
    }
}

// Define a function to get the current timestamp
pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|n| n.as_secs())
        .unwrap()
}

// Convert an incoming write request to a vector of internal metrics. Order of individual metrics is not currently preserved, however order of samples is preserved for a given metric.
pub fn from_write_request(
    write_request: WriteRequest,
    parse_config: &ParseConfig,
  ) -> (Vec<Metric>, Vec<ParseError>) {
    let mut errors = Vec::new();
    log::trace!(
      "received write request with {} metadata and {} timeseries",
      write_request.metadata.len(),
      write_request.timeseries.len()
    );
    let mut metric_type_map = write_request.metadata.into_iter().fold(
      HashMap::<Bytes, InProgressMetric>::default(),
      |mut map, m| {
        let metric_type: Option<MetricType> =
          prom_metric_type_to_internal_metric_type(m.type_, parse_config)
            .inspect_err(|_| {
              log::trace!(
                "family {} has invalid metric type: {:?}",
                m.metric_family_name,
                m.type_
              );
            })
            .ok();
        log::trace!(
          "found metric family: {}, type: {:?}",
          m.metric_family_name,
          metric_type
        );
        match map.entry(m.metric_family_name.into_bytes()) {
          Entry::Occupied(_) => {
            if !parse_config.ignore_duplicate_metadata {
              errors.push(ParseError::PromRemoteWrite(
                "duplicate metric family name in metadata",
              ));
            }
          },
          Entry::Vacant(v) => {
            v.insert(match metric_type {
              Some(MetricType::Histogram) => {
                InProgressMetric::Histogram(InProgressHistogram::default())
              },
              Some(MetricType::Summary) => InProgressMetric::Summary(InProgressSummary::default()),
              _ => InProgressMetric::Simple(metric_type),
            });
          },
        }
  
        map
      },
    );
  
    let mut metrics: Vec<_> = write_request
      .timeseries
      .into_iter()
      .flat_map(
        |time_series| match timeseries_to_metrics(time_series, &mut metric_type_map) {
          Ok(metrics) => metrics,
          Err(e) => {
            errors.push(e);
            vec![]
          },
        },
      )
      .collect();
  
    // Now we have to finalize any histograms that have been built.
    for (name, in_progress) in metric_type_map {
      match in_progress {
        InProgressMetric::Histogram(histogram) => {
          for (tags, samples) in histogram.histograms {
            for (timestamp, data) in samples {
              match finalize_histogram(&name, &tags, timestamp, data) {
                Ok(metric) => metrics.push(metric),
                Err(e) => errors.push(e),
              }
            }
          }
        },
        InProgressMetric::Summary(summary) => {
          for (tags, samples) in summary.summaries {
            for (timestamp, data) in samples {
              match finalize_summary(&name, &tags, timestamp, data) {
                Ok(metric) => metrics.push(metric),
                Err(e) => errors.push(e),
              }
            }
          }
        },
        InProgressMetric::Simple(_) => {},
      }
    }
  
    (metrics, errors)
  }
  


// Convert from otlp metric type to internal metric type.
pub fn otlp_metric_type_to_internal_metric_type(
    otlp_metric_type: EnumOrUnknown<OtlpMetricType>,
    parse_config: &ParseConfig,
  ) -> Result<MetricType, ParseError> {
    match otlp_metric_type
      .enum_value()
      .map_err(|_| ParseError::InvalidType)?
    {
      OtlpMetricType::COUNTER => {
        if parse_config.counter_as_delta {
          Ok(MetricType::Counter(CounterType::Delta))
        } else {
          Ok(MetricType::Counter(CounterType::Absolute))
        }
      },
      OtlpMetricType::DIRECTCOUNTER => Ok(MetricType::Counter(CounterType::Delta)),
      OtlpMetricType::DELTAGAUGE => Ok(MetricType::DeltaGauge),
      OtlpMetricType::DIRECTGAUGE => Ok(MetricType::DirectGauge),
      OtlpMetricType::GAUGE => Ok(MetricType::Gauge),
      OtlpMetricType::SUMMARY => {
        if parse_config.summary_as_timer {
          Ok(MetricType::Timer)
        } else {
          Ok(MetricType::Summary)
        }
      },
      OtlpMetricType::TIMER => Ok(MetricType::Timer),
      OtlpMetricType::BULKTIMER => Ok(MetricType::BulkTimer),
      OtlpMetricType::HISTOGRAM => Ok(MetricType::Histogram),
      _ => Err(ParseError::InvalidType),
    }
  }
  
  
  // Convert from internal metric type to otlp metric type.
  impl From<MetricType> for OtlpMetricType {
    fn from(t: MetricType) -> Self {
      #[allow(clippy::match_same_arms)]
      match t {
        MetricType::Counter(CounterType::Absolute) => Self::COUNTER,
        MetricType::Counter(CounterType::Delta) => Self::DIRECTCOUNTER,
        MetricType::DeltaGauge => Self::DELTAGAUGE,
        MetricType::DirectGauge => Self::DIRECTGAUGE,
        MetricType::Gauge => Self::GAUGE,
        MetricType::Histogram => Self::HISTOGRAM,
        MetricType::Timer => Self::TIMER,
        MetricType::Summary => Self::SUMMARY,
        MetricType::BulkTimer => Self::BULKTIMER,
      }
    }
  }


// Convert a timeseries to internal metrics.
fn timeseries_to_metrics(
    time_series: TimeSeries,
    metric_type_map: &mut HashMap<Bytes, InProgressMetric>,
  ) -> Result<Vec<Metric>, ParseError> {
    let mut name = Bytes::default();
    let mut tags = Vec::<TagValue>::with_capacity(time_series.labels.len() - 1);
    for label in time_series.labels {
      if &*label.name == "__name__" {
        name = label.value.into_bytes();
      } else {
        tags.push(TagValue {
          tag: label.name.into_bytes(),
          value: label.value.into_bytes(),
        });
      }
    }
  
    let mut ends_in_count = false;
    let mut ends_in_sum = false;
    let maybe_histogram = if name.ends_with(b"_bucket") {
      Some((
        &name[0 .. name.len() - "_bucket".len()],
        HistogramDataType::Bucket,
      ))
    } else if name.ends_with(b"_count") {
      ends_in_count = true;
      Some((
        &name[0 .. name.len() - "_count".len()],
        HistogramDataType::Count,
      ))
    } else if name.ends_with(b"_sum") {
      ends_in_sum = true;
      Some((
        &name[0 .. name.len() - "_sum".len()],
        HistogramDataType::Sum,
      ))
    } else {
      None
    };
  
    if let Some((family_name, data_type)) = maybe_histogram {
      if let Some(InProgressMetric::Histogram(histogram)) = metric_type_map.get_mut(family_name) {
        process_in_progress_histogram(histogram, data_type, tags, time_series.samples)?;
        return Ok(vec![]);
      }
    }
  
    let maybe_summary = if tags.iter().any(|t| t.tag.as_ref() == b"quantile") {
      Some((&name[..], SummaryDataType::Quantile))
    } else if ends_in_count {
      Some((
        &name[0 .. name.len() - "_count".len()],
        SummaryDataType::Count,
      ))
    } else if ends_in_sum {
      Some((&name[0 .. name.len() - "_sum".len()], SummaryDataType::Sum))
    } else {
      None
    };
  
    if let Some((family_name, data_type)) = maybe_summary {
      if let Some(InProgressMetric::Summary(summary)) = metric_type_map.get_mut(family_name) {
        process_in_progress_summary(summary, data_type, tags, time_series.samples)?;
        return Ok(vec![]);
      }
    }
  
    let mtype = match metric_type_map.get(&name) {
      None => {
        log::trace!(
          "prom metric name '{}' not found in metadata, defaulting type to None",
          String::from_utf8_lossy(&name)
        );
        None
      },
      Some(InProgressMetric::Simple(mtype)) => *mtype,
      Some(InProgressMetric::Histogram(_) | InProgressMetric::Summary(_)) => {
        return Err(ParseError::PromRemoteWrite(
          "invalid histogram or summary timeseries",
        ))
      },
    };
  
    let id = MetricId::new(name, mtype, tags, false)?;
    let res: Vec<Metric> = time_series
      .samples
      .into_iter()
      .filter_map(|s| {
        let metric_value = if Some(MetricType::BulkTimer) == id.mtype() {
          if s.bulk_values.is_empty() {
            warn_every!(
              15.seconds(),
              "ignoring prom sample with empty bulk values: {}",
              id
            );
            return None;
          }
  
          MetricValue::BulkTimer(s.bulk_values)
        } else {
          MetricValue::Simple(s.value)
        };
  
        Some(Metric::new(
          id.clone(),
          if s.sample_rate == 0. {
            None
          } else {
            Some(s.sample_rate)
          },
          unwrap_prom_timestamp(s.timestamp),
          metric_value,
        ))
      })
      .collect();
    Ok(res)
  }
  