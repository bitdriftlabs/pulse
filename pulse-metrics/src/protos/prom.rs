// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

// TODO(mattklein123): Write a fuzzer for this file.

#[cfg(test)]
#[path = "./prom_test.rs"]
mod prom_test;

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
use bd_proto::protos::prometheus::prompb;
use bytes::Bytes;
use config::inflow::v1::prom_remote_write::prom_remote_write_server_config::ParseConfig;
use itertools::Itertools;
use prompb::remote::WriteRequest;
use prompb::types::metric_metadata::MetricType as PromMetricType;
use prompb::types::{Label, MetricMetadata, Sample, TimeSeries};
use protobuf::{Chars, EnumOrUnknown};
use pulse_protobuf::protos::pulse::config;
use std::collections::hash_map::Entry;
use std::collections::BTreeMap;

type HashMap<Key, Value> = std::collections::HashMap<Key, Value, ahash::RandomState>;

// https://docs.google.com/document/d/1LPhVRSFkGNSuU1fBd81ulhsCPR4hkSZyyBj1SZ8fWOM/edit#heading=h.hfqkr2527w2g
const STALE_MARKER_BITS: u64 = 0x7ff0_0000_0000_0002;

// Convert stale marker bits to an f64. Note, this produces NaN which will never compare equal in
// Rust.
#[must_use]
pub const fn prom_stale_marker() -> f64 {
  f64::from_bits(STALE_MARKER_BITS)
}

// Because NaN never compares equal in Rust, in order to write tests we have to implement our own
// equality where it matters and consider two values that each contain the stale marker to be
// equal.
#[must_use]
pub fn f64_or_stale_marker_eq(lhs: f64, rhs: f64) -> bool {
  lhs == rhs || (lhs.to_bits() == STALE_MARKER_BITS && rhs.to_bits() == STALE_MARKER_BITS)
}

#[must_use]
pub fn make_label(name: Chars, value: Chars) -> Label {
  Label {
    name,
    value,
    ..Default::default()
  }
}

#[must_use]
pub fn make_timeseries(
  name: Chars,
  mut labels: Vec<Label>,
  value: f64,
  timestamp: i64,
  extra_labels: Vec<Label>,
) -> TimeSeries {
  labels.push(make_label("__name__".into(), name));
  labels.extend(extra_labels);
  labels.sort_unstable_by(|a, b| a.name.cmp(&b.name));
  TimeSeries {
    labels,
    samples: vec![Sample {
      value,
      timestamp,
      ..Default::default()
    }],
    ..Default::default()
  }
}

// Convert from prom metric type to internal metric type.
pub fn prom_metric_type_to_internal_metric_type(
  prom_metric_type: EnumOrUnknown<PromMetricType>,
  parse_config: &ParseConfig,
) -> Result<MetricType, ParseError> {
  match prom_metric_type
    .enum_value()
    .map_err(|_| ParseError::InvalidType)?
  {
    PromMetricType::COUNTER => {
      if parse_config.counter_as_delta {
        Ok(MetricType::Counter(CounterType::Delta))
      } else {
        Ok(MetricType::Counter(CounterType::Absolute))
      }
    },
    PromMetricType::DELTAGAUGE => Ok(MetricType::DeltaGauge),
    PromMetricType::DIRECTGAUGE => Ok(MetricType::DirectGauge),
    PromMetricType::GAUGE => Ok(MetricType::Gauge),
    PromMetricType::SUMMARY => {
      if parse_config.summary_as_timer {
        Ok(MetricType::Timer)
      } else {
        Ok(MetricType::Summary)
      }
    },
    // TODO(mattklein123): This is not correct. A real Prom/OTel state set is not the same as a
    // statsd aggregated set.
    PromMetricType::STATESET => Ok(MetricType::Set),
    PromMetricType::HISTOGRAM => Ok(MetricType::Histogram),
    _ => Err(ParseError::InvalidType),
  }
}


// Convert from internal metric type to prom metric type.
impl From<MetricType> for PromMetricType {
  fn from(t: MetricType) -> Self {
    #[allow(clippy::match_same_arms)]
    match t {
      MetricType::Counter(_) => Self::COUNTER,
      MetricType::DeltaGauge => Self::DELTAGAUGE,
      MetricType::DirectGauge => Self::DIRECTGAUGE,
      MetricType::Gauge => Self::GAUGE,
      MetricType::Histogram => Self::HISTOGRAM,
      // TODO(mattklein123): This should only be true when the Lyft summary hack is enabled, but we
      // don't currently configure that on outflow. Reconcile this later.
      MetricType::Timer => Self::SUMMARY,
      // TODO(mattklein123): Not correct. See above.
      MetricType::Set => Self::STATESET,
      MetricType::Summary => Self::SUMMARY,
    }
  }
}

//
// InProgressHistogramData
//

// A histogram that is currently being built from incoming data.
#[derive(Default, Debug)]
struct InProgressHistogramData {
  buckets: Vec<HistogramBucket>,
  sample_count: Option<f64>,
  sample_sum: Option<f64>,
  saw_inf: bool,
  saw_count: bool,
}

//
// HistogramDataType
//

// For incoming histogram data being parsed, whether it refers to bucket info, total count, or
// total sum
#[derive(Clone, Copy)]
enum HistogramDataType {
  Bucket,
  Count,
  Sum,
}

//
// InProgressHistogram
//

// Tracks a complete histogram being parsed, including common tags, as well as data for every
// incoming timestamp.
#[derive(Default, Debug)]
struct InProgressHistogram {
  histograms: HashMap<
    Vec<TagValue>,
    // BTreeMap is used here so that timestamps are sorted, regardless of input order.
    // TODO(mattklein123): Technically the "spec" says they should be in timestamp order so we
    // could probably just fail if out of order but we do this for now.
    BTreeMap<i64, InProgressHistogramData>,
  >,
}

//
// SummaryDataType
//

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

//
// InProgressMetric
//

// A metric family currently being parsed. "Simple" metrics only need the type, however histograms
// are parsed across multiple time series so we have to maintain state during ingestion.
enum InProgressMetric {
  Simple(Option<MetricType>),
  Histogram(InProgressHistogram),
  Summary(InProgressSummary),
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

// Given the tags and samples for an incoming timeseries that is part of a histogram, collect the
// samples into the in progress state.
fn process_in_progress_histogram(
  histogram: &mut InProgressHistogram,
  data_type: HistogramDataType,
  mut tags: Vec<TagValue>,
  samples: Vec<Sample>,
) -> Result<(), ParseError> {
  let (new_data_type, maybe_le) = match data_type {
    HistogramDataType::Bucket => {
      // Find the "le" tag, and fail if it does not exist.
      let (le_tag_index, le_tag) = tags
        .iter()
        .enumerate()
        .find(|(_, tag)| tag.tag == "le")
        .ok_or(ParseError::PromRemoteWrite("missing le tag"))?;

      // Special case the +Inf bucket.
      if le_tag.value.as_ref() == b"+Inf" {
        tags.swap_remove(le_tag_index);
        (HistogramDataType::Count, None)
      } else {
        let le = std::str::from_utf8(&le_tag.value)
          .map_err(|_| ParseError::PromRemoteWrite("invalid le tag"))?
          .parse::<f64>()
          .map_err(|_| ParseError::PromRemoteWrite("invalid le tag"))?;
        if !le.is_finite() {
          return Err(ParseError::PromRemoteWrite("invalid le tag"));
        }
        tags.swap_remove(le_tag_index);
        (HistogramDataType::Bucket, Some(le))
      }
    },
    HistogramDataType::Count | HistogramDataType::Sum => (data_type, None),
  };

  // At this point, the "le" tag has been removed if applicable. Now sort the remaining tags so we
  // have a stable hash key.
  tags.sort_unstable();
  let timeseries = histogram.histograms.entry(tags).or_default();

  // Push the samples into the relevant entry for the given timestamp.
  for sample in samples {
    let data = timeseries.entry(sample.timestamp).or_default();
    match new_data_type {
      HistogramDataType::Bucket => {
        data.buckets.push(HistogramBucket {
          le: maybe_le.unwrap(),
          count: sample.value,
        });
      },
      HistogramDataType::Count => {
        if matches!(data_type, HistogramDataType::Bucket) {
          if data.saw_inf {
            return Err(ParseError::PromRemoteWrite("duplicate +Inf"));
          }
          data.saw_inf = true;
        } else {
          if data.saw_count {
            return Err(ParseError::PromRemoteWrite("duplicate _count"));
          }
          data.saw_count = true;
        }

        if data.sample_count.map_or(false, |current_sample_count| {
          current_sample_count != sample.value
        }) {
          return Err(ParseError::PromRemoteWrite("mismatch +Inf and _count"));
        }

        data.sample_count = Some(sample.value);
      },
      HistogramDataType::Sum => {
        if data.sample_sum.is_some() {
          return Err(ParseError::PromRemoteWrite("duplicate _sum"));
        }
        data.sample_sum = Some(sample.value);
      },
    }
  }

  Ok(())
}

// Given in progress summary data, attempt to finalize and fail if not possible.
fn in_progress_summary_data_to_summary_data(
  mut in_progress: InProgressSummaryData,
) -> Result<SummaryData, ParseError> {
  if in_progress.quantiles.is_empty() {
    return Err(ParseError::PromRemoteWrite("empty quantiles"));
  }

  in_progress
    .quantiles
    .sort_unstable_by(|a, b| a.quantile.partial_cmp(&b.quantile).unwrap());
  for i in 0 .. in_progress.quantiles.len() {
    if i == 0 {
      continue;
    }
    let current = &in_progress.quantiles[i];
    let previous = &in_progress.quantiles[i - 1];
    // Make sure that quantile is > previous quantile (checks for duplicates after the sort).
    if current.quantile <= previous.quantile {
      return Err(ParseError::PromRemoteWrite("duplicate quantile value"));
    }
  }

  let Some(sample_count) = in_progress.sample_count else {
    return Err(ParseError::PromRemoteWrite("sample count not provided"));
  };
  let Some(sample_sum) = in_progress.sample_sum else {
    return Err(ParseError::PromRemoteWrite("sample sum not provided"));
  };

  Ok(SummaryData {
    quantiles: in_progress.quantiles,
    sample_count,
    sample_sum,
  })
}

// Given in progress histogram data, attempt to finalize and fail if not possible.
fn in_progress_histogram_data_to_histogram_data(
  mut in_progress: InProgressHistogramData,
) -> Result<HistogramData, ParseError> {
  if !in_progress.saw_count || !in_progress.saw_inf {
    return Err(ParseError::PromRemoteWrite("_count or +Inf not provided"));
  }

  if in_progress.buckets.is_empty() {
    return Err(ParseError::PromRemoteWrite("empty buckets"));
  }

  // The buckets might have come in different orders. Sort them by le.
  in_progress
    .buckets
    .sort_unstable_by(|a, b| a.le.partial_cmp(&b.le).unwrap());
  for i in 0 .. in_progress.buckets.len() {
    if i == 0 {
      continue;
    }
    let current = &in_progress.buckets[i];
    let previous = &in_progress.buckets[i - 1];
    // Make sure that le is > previous le (checks for duplicates after the sort).
    if current.le <= previous.le {
      return Err(ParseError::PromRemoteWrite("duplicate le value"));
    }
    // Make sure count >= previous count.
    if current.count < previous.count {
      return Err(ParseError::PromRemoteWrite("counts not ascending"));
    }
  }

  let sample_count = in_progress.sample_count.unwrap();
  if sample_count < in_progress.buckets[in_progress.buckets.len() - 1].count {
    return Err(ParseError::PromRemoteWrite("sample count < final bucket"));
  }

  Ok(HistogramData {
    buckets: in_progress.buckets,
    sample_count: in_progress.sample_count.unwrap(),
    sample_sum: in_progress
      .sample_sum
      .ok_or(ParseError::PromRemoteWrite("sample sum not provided"))?,
  })
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
    None => None,
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
    .iter()
    .map(|s| {
      Metric::new(
        id.clone(),
        if s.sample_rate == 0. {
          None
        } else {
          Some(s.sample_rate)
        },
        unwrap_prom_timestamp(s.timestamp),
        MetricValue::Simple(s.value),
      )
    })
    .collect();
  Ok(res)
}

// Finalize an in progress histogram.
fn finalize_histogram(
  name: &Bytes,
  tags: &[TagValue],
  timestamp: i64,
  data: InProgressHistogramData,
) -> Result<Metric, ParseError> {
  Ok(Metric::new(
    MetricId::new(
      name.clone(),
      Some(MetricType::Histogram),
      tags.to_vec(),
      true,
    )?,
    None,
    unwrap_prom_timestamp(timestamp),
    MetricValue::Histogram(in_progress_histogram_data_to_histogram_data(data)?),
  ))
}

// Finalize an in progress summary.
fn finalize_summary(
  name: &Bytes,
  tags: &[TagValue],
  timestamp: i64,
  data: InProgressSummaryData,
) -> Result<Metric, ParseError> {
  Ok(Metric::new(
    MetricId::new(name.clone(), Some(MetricType::Summary), tags.to_vec(), true)?,
    None,
    unwrap_prom_timestamp(timestamp),
    MetricValue::Summary(in_progress_summary_data_to_summary_data(data)?),
  ))
}

// Convert an incoming write request to a vector of internal metrics. Order of individual metrics
// is not currently preserved, however order of samples is preserved for a given metric.
pub fn from_write_request(
  write_request: WriteRequest,
  parse_config: &ParseConfig,
) -> (Vec<Metric>, Vec<ParseError>) {
  let mut errors = Vec::new();
  let mut metric_type_map = write_request.metadata.into_iter().fold(
    HashMap::<Bytes, InProgressMetric>::default(),
    |mut map, m| {
      let metric_type: Option<MetricType> =
        prom_metric_type_to_internal_metric_type(m.type_, parse_config).ok();
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

// Normalize an incoming Prom timestamp.
fn unwrap_prom_timestamp(timestamp: i64) -> u64 {
  unwrap_timestamp(
    if timestamp == 0 {
      None
    } else {
      Some((timestamp / 1000) as u64)
    },
  )
}

// Given an internal metric, create the prom metric family name.
fn make_family_name(metric: &ParsedMetric, options: &ToWriteRequestOptions) -> Chars {
  let metric_name_to_use = if options.convert_name {
    match metric.source() {
      MetricSource::Carbon(_)
      | MetricSource::Statsd(_)
      | MetricSource::Aggregation { prom_source: false } => metric.metric().get_id().prom_name(),
      MetricSource::PromRemoteWrite | MetricSource::Aggregation { prom_source: true } => {
        metric.metric().get_id().name().clone()
      },
    }
  } else {
    metric.metric().get_id().name().clone()
  };
  Chars::from_bytes(metric_name_to_use).unwrap_or_default()
}

// Common timeseries creation for both simple metrics and histograms.
fn timeseries_common(
  timeseries_map: &mut HashMap<Vec<TagValue>, TimeSeries>,
  tags: Vec<TagValue>,
  value: f64,
  timestamp: u64,
  sample_rate: f64,
  options: &ToWriteRequestOptions,
) {
  let timeseries = timeseries_map.entry(tags).or_insert_with_key(|tags| {
    let labels: Vec<Label> = tags
      .iter()
      .map(|tag| {
        make_label(
          Chars::from_bytes(tag.tag.clone()).unwrap_or_default(),
          Chars::from_bytes(tag.value.clone()).unwrap_or_default(),
        )
      })
      .collect();

    TimeSeries {
      labels,
      ..Default::default()
    }
  });

  if !matches!(options.metadata, MetadataType::Only) {
    timeseries.samples.push(Sample {
      value,
      timestamp: (timestamp * 1000) as i64,
      sample_rate,
      ..Default::default()
    });
  }
}

// Expand a given summary into multiple component timeseries.
fn timeseries_for_summary_metric(
  metric: &ParsedMetric,
  timeseries_map: &mut HashMap<Vec<TagValue>, TimeSeries>,
  metadata_map: &mut HashMap<Chars, PromMetricType>,
  options: &ToWriteRequestOptions,
) {
  let family_name = make_family_name(metric, options);
  metadata_map.insert(family_name.clone(), PromMetricType::SUMMARY);
  let summary = metric.metric().value.to_summary();
  for bucket in &summary.quantiles {
    timeseries_common(
      timeseries_map,
      final_tags_for_timeseries(
        metric.metric().get_id().tags().to_vec(),
        family_name.clone(),
        Some(TagValue {
          tag: "quantile".into(),
          value: bucket.quantile.to_string().into(),
        }),
      ),
      bucket.value,
      metric.metric().timestamp,
      metric.metric().sample_rate.unwrap_or_default(),
      options,
    );
  }
  timeseries_common(
    timeseries_map,
    final_tags_for_timeseries(
      metric.metric().get_id().tags().to_vec(),
      format!("{family_name}_count").into(),
      None,
    ),
    summary.sample_count,
    metric.metric().timestamp,
    metric.metric().sample_rate.unwrap_or_default(),
    options,
  );
  timeseries_common(
    timeseries_map,
    final_tags_for_timeseries(
      metric.metric().get_id().tags().to_vec(),
      format!("{family_name}_sum").into(),
      None,
    ),
    summary.sample_sum,
    metric.metric().timestamp,
    metric.metric().sample_rate.unwrap_or_default(),
    options,
  );
}

// Expand a given histogram into multiple component timeseries.
fn timeseries_for_histogram_metric(
  metric: &ParsedMetric,
  timeseries_map: &mut HashMap<Vec<TagValue>, TimeSeries>,
  metadata_map: &mut HashMap<Chars, PromMetricType>,
  options: &ToWriteRequestOptions,
) {
  let family_name = make_family_name(metric, options);
  metadata_map.insert(family_name.clone(), PromMetricType::HISTOGRAM);
  let histogram = metric.metric().value.to_histogram();
  for bucket in &histogram.buckets {
    timeseries_common(
      timeseries_map,
      final_tags_for_timeseries(
        metric.metric().get_id().tags().to_vec(),
        format!("{family_name}_bucket").into(),
        Some(TagValue {
          tag: "le".into(),
          value: bucket.le.to_string().into(),
        }),
      ),
      bucket.count,
      metric.metric().timestamp,
      metric.metric().sample_rate.unwrap_or_default(),
      options,
    );
  }
  timeseries_common(
    timeseries_map,
    final_tags_for_timeseries(
      metric.metric().get_id().tags().to_vec(),
      format!("{family_name}_bucket").into(),
      Some(TagValue {
        tag: "le".into(),
        value: "+Inf".into(),
      }),
    ),
    histogram.sample_count,
    metric.metric().timestamp,
    metric.metric().sample_rate.unwrap_or_default(),
    options,
  );
  timeseries_common(
    timeseries_map,
    final_tags_for_timeseries(
      metric.metric().get_id().tags().to_vec(),
      format!("{family_name}_count").into(),
      None,
    ),
    histogram.sample_count,
    metric.metric().timestamp,
    metric.metric().sample_rate.unwrap_or_default(),
    options,
  );
  timeseries_common(
    timeseries_map,
    final_tags_for_timeseries(
      metric.metric().get_id().tags().to_vec(),
      format!("{family_name}_sum").into(),
      None,
    ),
    histogram.sample_sum,
    metric.metric().timestamp,
    metric.metric().sample_rate.unwrap_or_default(),
    options,
  );
}

// Create the final tags for an outgoing timeseries.
fn final_tags_for_timeseries(
  mut tags: Vec<TagValue>,
  name: Chars,
  extra_tag: Option<TagValue>,
) -> Vec<TagValue> {
  tags.push(TagValue {
    tag: "__name__".into(),
    value: name.into_bytes(),
  });
  if let Some(extra_tag) = extra_tag {
    tags.push(extra_tag);
  }
  tags.sort_unstable();
  tags
}

// Create the timeseries for a "simple" (non-histogram) metric.
fn timeseries_for_simple_metric(
  metric: &ParsedMetric,
  timeseries_map: &mut HashMap<Vec<TagValue>, TimeSeries>,
  metadata_map: &mut HashMap<Chars, PromMetricType>,
  options: &ToWriteRequestOptions,
) {
  let family_name = make_family_name(metric, options);
  metadata_map.insert(
    family_name.clone(),
    metric
      .metric()
      .get_id()
      .mtype()
      .map_or(PromMetricType::UNKNOWN, std::convert::Into::into),
  );

  timeseries_common(
    timeseries_map,
    final_tags_for_timeseries(metric.metric().get_id().tags().to_vec(), family_name, None),
    metric.metric().value.to_simple(),
    metric.metric().timestamp,
    metric.metric().sample_rate.unwrap_or_default(),
    options,
  );
}

pub enum MetadataType {
  Normal,
  Only,
  None,
}

pub struct ToWriteRequestOptions {
  pub metadata: MetadataType,
  pub convert_name: bool,
}

// Converts a list of metrics into a Prometheus remote write request. If `metadata_only` is true,
// then only the metric metadata (metric labels and types) are included.
pub fn to_write_request(metrics: &[ParsedMetric], options: &ToWriteRequestOptions) -> WriteRequest {
  let mut metadata_map: HashMap<Chars, PromMetricType> = HashMap::default();
  let mut timeseries_map: HashMap<Vec<TagValue>, TimeSeries> = HashMap::default();

  for metric in metrics {
    match metric.metric().get_id().mtype() {
      Some(MetricType::Histogram) => {
        timeseries_for_histogram_metric(metric, &mut timeseries_map, &mut metadata_map, options);
      },
      Some(MetricType::Summary) => {
        timeseries_for_summary_metric(metric, &mut timeseries_map, &mut metadata_map, options);
      },
      _ => timeseries_for_simple_metric(metric, &mut timeseries_map, &mut metadata_map, options),
    }
  }

  let metadata: Vec<MetricMetadata> = if matches!(options.metadata, MetadataType::None) {
    vec![]
  } else {
    metadata_map
      .into_iter()
      .map(|(metric_family_name, type_)| MetricMetadata {
        type_: type_.into(),
        metric_family_name,
        ..Default::default()
      })
      .collect()
  };

  WriteRequest {
    timeseries: timeseries_map.into_values().collect(),
    metadata,
    ..Default::default()
  }
}
