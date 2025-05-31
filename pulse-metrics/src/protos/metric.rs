// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./metric_test.rs"]
mod metric_test;

use super::carbon::to_carbon_line;
use super::prom::{
  ChangedTypeTracker,
  ToWriteRequestOptions,
  f64_or_stale_marker_eq,
  from_write_request,
  to_write_request,
};
use super::statsd::to_statsd_line;
use crate::pipeline::metric_cache::{CachedMetric, MetricCache};
use bd_proto::protos::prometheus::prompb::remote::WriteRequest;
use bytes::Bytes;
use config::common::v1::common::WireProtocol;
use config::common::v1::common::wire_protocol::Protocol_type;
use config::inflow::v1::prom_remote_write::prom_remote_write_server_config::ParseConfig;
use protobuf::Chars;
use pulse_common::metadata::Metadata;
use pulse_protobuf::protos::pulse::config;
use std::fmt::Display;
use std::hash::Hash;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use time::OffsetDateTime;

// Specifies whether a counter is a delta counter or a Prometheus style absolute counter.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CounterType {
  Delta,
  Absolute,
}

//
// MetricType
//

// Internal metric type common across different formats. Will drive what type of MetricValue is
// expected inside the associated Metric.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MetricType {
  Counter(CounterType),
  DeltaGauge,
  DirectGauge,
  Gauge,
  Histogram,
  Summary,
  Timer,
  BulkTimer,
}

impl MetricType {
  pub const fn from_statsd(t: &[u8]) -> Result<Self, ParseError> {
    match t {
      b"c" => Ok(Self::Counter(CounterType::Delta)),
      b"k" | b"G" => Ok(Self::DirectGauge),
      b"g" => Ok(Self::Gauge),
      b"h" | b"ms" => Ok(Self::Timer),
      _ => Err(ParseError::InvalidType),
    }
  }

  #[must_use]
  pub fn to_statsd(&self) -> &'static [u8] {
    match self {
      // TODO(mattklein123): We should block absolute counters at this level as they have no
      // statsd meaning.
      Self::Counter(_) => b"c",
      Self::DeltaGauge | Self::Gauge => b"g",
      Self::DirectGauge => b"G",
      // TODO(mattklein123): Blocked at the wire outflow level.
      Self::Histogram | Self::Summary | Self::BulkTimer => unreachable!(),
      Self::Timer => b"ms",
    }
  }
}

//
// TagValue
//

// Wraps a metric tag (key, value) across different formats.
#[derive(PartialOrd, Eq, Ord, Debug, Clone, PartialEq, Hash)]
pub struct TagValue {
  pub tag: bytes::Bytes,
  pub value: bytes::Bytes,
}

impl Display for TagValue {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      f,
      "{}={}",
      String::from_utf8_lossy(&self.tag),
      String::from_utf8_lossy(&self.value)
    )
  }
}

//
// MetricId
//

// Wraps a metric ID used for hashing and equality, including name, type, and sorted tags.
#[derive(Clone, Debug, Eq, PartialOrd, PartialEq)]
pub struct MetricId {
  name: bytes::Bytes,
  mtype: Option<MetricType>,
  tags: Vec<TagValue>,
}

fn tags_sorted(tags: &[TagValue]) -> bool {
  let mut cloned_tags = tags.to_vec();
  cloned_tags.sort_unstable();
  tags == cloned_tags
}

impl MetricId {
  // Create a new metric ID, making sure tags are sorted so that we have proper equivalence.
  // Some code paths assume that the tags are already sorted so we can indicate that here to
  // save some computation.
  pub fn new(
    name: bytes::Bytes,
    mtype: Option<MetricType>,
    mut tags: Vec<TagValue>,
    already_sorted: bool,
  ) -> Result<Self, ParseError> {
    // Right now the MetricKey representation uses length prefixed packed members with a max size
    // of u16, so check for that here.
    if name.len() > u16::MAX as usize
      || tags
        .iter()
        .any(|t| t.tag.len() > u16::MAX as usize || t.value.len() > u16::MAX as usize)
    {
      return Err(ParseError::TooLarge);
    }

    if already_sorted {
      debug_assert!(tags_sorted(&tags));
    } else {
      tags.sort_unstable();
    }
    Ok(Self { name, mtype, tags })
  }

  pub const fn mtype(&self) -> Option<MetricType> {
    self.mtype
  }

  pub fn set_mtype(&mut self, mtype: MetricType) {
    self.mtype = Some(mtype);
  }

  pub const fn name(&self) -> &bytes::Bytes {
    &self.name
  }

  pub fn tags(&self) -> &[TagValue] {
    &self.tags
  }

  pub fn tag(&self, tag_name: &str) -> Option<&TagValue> {
    self
      .tags
      .binary_search_by(|t| t.tag.as_ref().cmp(tag_name.as_bytes()))
      .ok()
      .map(|i| &self.tags[i])
  }

  pub fn into_parts(self) -> (bytes::Bytes, Option<MetricType>, Vec<TagValue>) {
    (self.name, self.mtype, self.tags)
  }
}

// This is written out to explicitly match the implementation in MetricKey.
impl Hash for MetricId {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.name.hash(state);
    self.mtype.hash(state);
    for tag in &self.tags {
      tag.tag.hash(state);
      tag.value.hash(state);
    }
  }
}

impl std::fmt::Display for MetricId {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let name_str = String::from_utf8_lossy(self.name.as_ref());
    write!(f, "{name_str}(")?;
    for tag in &self.tags {
      let tagk = String::from_utf8_lossy(tag.tag.as_ref());
      let tagv = String::from_utf8_lossy(tag.value.as_ref());
      write!(f, "[{tagk}={tagv}]")?;
    }
    write!(f, ")")
  }
}

//
// HistogramData
//

// Histogram data for aggregated histogram values.
#[derive(Clone, Debug, Default)]
pub struct HistogramBucket {
  pub le: f64,
  pub count: f64,
}
#[derive(Clone, Debug, Default)]
pub struct HistogramData {
  pub buckets: Vec<HistogramBucket>,
  pub sample_count: f64,
  pub sample_sum: f64,
}

// Need to implement equality to account for prometheus stale markers which are NaN.
impl PartialEq for HistogramData {
  fn eq(&self, other: &Self) -> bool {
    if self.buckets.len() != other.buckets.len() {
      return false;
    }

    self
      .buckets
      .iter()
      .zip(other.buckets.iter())
      .all(|(lhs, rhs)| f64_or_stale_marker_eq(lhs.count, rhs.count) && lhs.le == rhs.le)
      && f64_or_stale_marker_eq(self.sample_count, other.sample_count)
      && f64_or_stale_marker_eq(self.sample_sum, other.sample_sum)
  }
}

//
// SummaryData
//

// Summary data for aggregated summary values.
#[derive(Clone, Debug, Default)]
pub struct SummaryBucket {
  pub quantile: f64,
  pub value: f64,
}
#[derive(Clone, Debug, Default)]
pub struct SummaryData {
  pub quantiles: Vec<SummaryBucket>,
  pub sample_count: f64,
  pub sample_sum: f64,
}

// Need to implement equality to account for prometheus stale markers which are NaN.
impl PartialEq for SummaryData {
  fn eq(&self, other: &Self) -> bool {
    if self.quantiles.len() != other.quantiles.len() {
      return false;
    }

    self
      .quantiles
      .iter()
      .zip(other.quantiles.iter())
      .all(|(lhs, rhs)| {
        f64_or_stale_marker_eq(lhs.value, rhs.value) && lhs.quantile == rhs.quantile
      })
      && f64_or_stale_marker_eq(self.sample_count, other.sample_count)
      && f64_or_stale_marker_eq(self.sample_sum, other.sample_sum)
  }
}

//
// MetricValue
//

// Wraps a metric value, which depends op the associated MetricType in the MetridId.
#[derive(Clone, Debug)]
pub enum MetricValue {
  Simple(f64),
  Histogram(HistogramData),
  Summary(SummaryData),
  BulkTimer(Vec<f64>),
}

// Need to implement equality to account for prometheus stale markers which are NaN.
impl PartialEq for MetricValue {
  fn eq(&self, other: &Self) -> bool {
    match (self, other) {
      (Self::Simple(lhs), Self::Simple(rhs)) => f64_or_stale_marker_eq(*lhs, *rhs),
      (Self::Histogram(lhs), Self::Histogram(rhs)) => lhs == rhs,
      (Self::Summary(lhs), Self::Summary(rhs)) => lhs == rhs,
      (Self::BulkTimer(lhs), Self::BulkTimer(rhs)) => lhs == rhs,
      _ => false,
    }
  }
}

impl MetricValue {
  #[must_use]
  pub fn to_simple(&self) -> f64 {
    match self {
      Self::Simple(value) => *value,
      Self::Histogram(_) | Self::Summary(_) | Self::BulkTimer(_) => unreachable!(),
    }
  }

  #[must_use]
  pub fn to_histogram(&self) -> &HistogramData {
    match self {
      Self::Simple(_) | Self::Summary(_) | Self::BulkTimer(_) => unreachable!(),
      Self::Histogram(h) => h,
    }
  }

  #[must_use]
  pub fn to_summary(&self) -> &SummaryData {
    match self {
      Self::Simple(_) | Self::Histogram(_) | Self::BulkTimer(_) => unreachable!(),
      Self::Summary(s) => s,
    }
  }

  #[must_use]
  pub fn to_bulk_timer(&self) -> &[f64] {
    match self {
      Self::Simple(_) | Self::Histogram(_) | Self::Summary(_) => unreachable!(),
      Self::BulkTimer(b) => b,
    }
  }

  #[must_use]
  pub fn into_histogram(self) -> HistogramData {
    match self {
      Self::Simple(_) | Self::Summary(_) | Self::BulkTimer(_) => unreachable!(),
      Self::Histogram(h) => h,
    }
  }

  #[must_use]
  pub fn into_summary(self) -> SummaryData {
    match self {
      Self::Simple(_) | Self::Histogram(_) | Self::BulkTimer(_) => unreachable!(),
      Self::Summary(s) => s,
    }
  }

  #[must_use]
  pub fn into_bulk_timer(self) -> Vec<f64> {
    match self {
      Self::Simple(_) | Self::Histogram(_) | Self::Summary(_) => unreachable!(),
      Self::BulkTimer(b) => b,
    }
  }
}

//
// Metric
//

// A metric, composed of an ID, a sample rate, a timestamp, and a value.
#[derive(Clone, Debug, PartialEq)]
pub struct Metric {
  id: MetricId,
  pub sample_rate: Option<f64>,
  pub timestamp: u64,
  pub value: MetricValue,
}

const MAX_SECONDS_TIMESTAMP: u64 = 100_000_000_000;

const fn normalize_timestamp(t: u64) -> u64 {
  if t > MAX_SECONDS_TIMESTAMP {
    t / 1000
  } else {
    t
  }
}

pub fn unwrap_timestamp(otimestamp: Option<u64>) -> u64 {
  otimestamp.map_or_else(default_timestamp, normalize_timestamp)
}

#[must_use]
pub fn default_timestamp() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|n| n.as_secs())
    .unwrap()
}

impl Metric {
  pub const fn new(
    id: MetricId,
    sample_rate: Option<f64>,
    timestamp: u64,
    value: MetricValue,
  ) -> Self {
    Self {
      id,
      sample_rate,
      timestamp,
      value,
    }
  }

  pub fn into_parts(self) -> (MetricId, Option<f64>, u64, MetricValue) {
    (self.id, self.sample_rate, self.timestamp, self.value)
  }

  pub const fn get_id(&self) -> &MetricId {
    &self.id
  }

  pub fn set_id(&mut self, id: MetricId) {
    self.id = id;
  }

  pub fn to_datetime(&self) -> Option<OffsetDateTime> {
    OffsetDateTime::from_unix_timestamp(i64::try_from(self.timestamp).unwrap()).ok()
  }

  pub fn to_wire_format(&self, wire_protocol: &WireProtocol) -> bytes::Bytes {
    match wire_protocol.protocol_type {
      Some(Protocol_type::Statsd(_)) => to_statsd_line(self),
      Some(Protocol_type::Carbon(_)) => to_carbon_line(self),
      None => unreachable!("pgv"),
    }
  }
}

impl std::fmt::Display for Metric {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      f,
      "{}[VALUE={}][TIMESTAMP={}]",
      self.id,
      match self.value {
        MetricValue::Simple(s) => s.to_string(),
        MetricValue::Histogram(_) => "histogram".to_string(),
        MetricValue::Summary(_) => "summary".to_string(),
        MetricValue::BulkTimer(_) => "bulk_timer".to_string(),
      },
      self.timestamp,
    )
  }
}

//
// ParseError
//

// Errors that arise during parsing.
#[derive(Error, Debug, Eq, PartialEq)]
pub enum ParseError {
  #[error("generic parse error")]
  Generic,
  #[error("invalid parsed value")]
  InvalidValue,
  #[error("invalid sample rate")]
  InvalidSampleRate,
  #[error("invalid type")]
  InvalidType,
  #[error("invalid tag")]
  InvalidTag,
  #[error("overall invalid line - no structural elements found in parsing")]
  InvalidLine,
  #[error("invalid protocol")]
  InvalidProtocol,
  #[error("prometheus remote write error: {0}")]
  PromRemoteWrite(String),
  #[error("more than one sample rate field found")]
  RepeatedSampleRate,
  #[error("more than one set of tags found")]
  RepeatedTags,
  #[error("name, tag name, or tag value length too large")]
  TooLarge,
  #[error("unsupported extension field")]
  UnsupportedExtensionField,
  #[error("cannot change protocol for unparsable metric sample")]
  UnparsableMetricChangeProtocol,
  #[error("invalid timestamp")]
  InvalidTimestamp,
}

//
// MetricSource
//

// The source of a metric.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetricSource {
  Carbon(bytes::Bytes),
  Statsd(bytes::Bytes),
  PromRemoteWrite,
  Otlp,
  Aggregation { prom_source: bool },
}

//
// DownstreamId
//

// The ID of the downstream sender of a metric.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DownstreamId {
  // The metric originated from within this process.
  LocalOrigin,
  // The metric originated from a remote IP address. Only IP is currently considered as remote
  // port is unlikely to be stable across reconnections.
  IpAddress(IpAddr),
  // The metric originated from a unix domain socket with the specified name.
  UnixDomainSocket(Chars),
  // An inflow specific ID. See inflow configuration documentation for more information.
  InflowProvided(Bytes),
}

impl DownstreamId {
  // Create a "deep" clone suitable for hash key storage (does not contain references to
  // potentially large network payloads).
  #[must_use]
  pub fn clone_for_hash_key(&self) -> Self {
    match self {
      Self::LocalOrigin => Self::LocalOrigin,
      Self::UnixDomainSocket(name) => Self::UnixDomainSocket(name.to_string().into()),
      Self::IpAddress(address) => Self::IpAddress(*address),
      Self::InflowProvided(inflow_provided) => {
        Self::InflowProvided(inflow_provided.to_vec().into())
      },
    }
  }
}

//
// DownstreamIdProvider
//

pub trait DownstreamIdProvider {
  fn downstream_id(&self, metric_id: &MetricId) -> DownstreamId;
}

//
// EditableParsedMetric
//

// A wrapper for a ParsedMetric that allows safely changing the metric name and tags. This is
// required because if either the name/tags are changed, any cached metric reference needs to
// be invalidated. If tags are added, they need to be resorted.
#[derive(Debug)]
pub struct EditableParsedMetric<'a> {
  metric: &'a mut ParsedMetric,
  tag_insertion_index: Option<usize>,
  name_changed: bool,
  deleted_tags: Option<Vec<bool>>,
}

impl<'a> EditableParsedMetric<'a> {
  pub fn new(metric: &'a mut ParsedMetric) -> Self {
    Self {
      metric,
      tag_insertion_index: None,
      name_changed: false,
      deleted_tags: None,
    }
  }

  pub fn assign_tags(&mut self, mut tags: Vec<TagValue>) {
    // Just sort now so we effectively do a full reset. In the common case scripts are not going
    // to do this and then do more edits.
    tags.sort_unstable();
    self.metric.metric.id.tags = tags;
    self.tag_insertion_index = None;
    self.deleted_tags = None;
  }

  pub fn add_or_change_tag(&mut self, tag: TagValue) {
    log::trace!("adding or changing tag: {tag}");
    if let Some(existing_tag) = self
      .find_tag_inner(&tag.tag, true)
      .map(|index| &mut self.metric.metric.id.tags[index])
    {
      existing_tag.value = tag.value;
    } else {
      self.metric.metric.id.tags.push(tag);
      if self.tag_insertion_index.is_none() {
        self.tag_insertion_index = Some(self.metric.metric.id.tags.len() - 1);
      }
      if let Some(deleted_tags) = &mut self.deleted_tags {
        deleted_tags.push(false);
      }
    }
  }

  pub fn find_tag(&mut self, tag_name: &[u8]) -> Option<&mut TagValue> {
    self
      .find_tag_inner(tag_name, false)
      .map(|index| &mut self.metric.metric.id.tags[index])
  }

  pub fn delete_tag(&mut self, tag_name: &[u8]) -> Option<Bytes> {
    if let Some(index) = self.find_tag_inner(tag_name, false) {
      let deleted_tags = self
        .deleted_tags
        .get_or_insert_with(|| vec![false; self.metric.metric.id.tags.len()]);
      deleted_tags[index] = true;
      log::trace!(
        "tag '{}' marked for deletion",
        self.metric.metric.id.tags[index]
      );
      Some(self.metric.metric.id.tags[index].value.clone())
    } else {
      None
    }
  }

  fn tag_deleted(
    tags: &[TagValue],
    deleted_tags: &mut Option<Vec<bool>>,
    index: usize,
    undelete: bool,
  ) -> bool {
    let deleted = deleted_tags.as_mut().is_some_and(|deleted_tags| {
      if undelete {
        log::trace!("tag '{}' was undeleted", tags[index]);
        deleted_tags[index] = false;
      }

      deleted_tags[index]
    });

    if deleted {
      log::trace!("tag '{}' was deleted", tags[index]);
    }

    deleted
  }

  fn find_tag_inner(&mut self, tag_name: &[u8], undelete: bool) -> Option<usize> {
    let tags = &self.metric.metric.id.tags;
    let tag_insertion_index = self.tag_insertion_index.unwrap_or(tags.len());

    // Anything that we had before we started should already be sorted, so we can binary
    // search to find the tag.
    let sorted_tags = &tags[0 .. tag_insertion_index];
    debug_assert!(tags_sorted(sorted_tags));
    if let Some(index) = sorted_tags
      .binary_search_by(|t| t.tag.as_ref().cmp(tag_name))
      .ok()
      .inspect(|index| log::trace!("found tag '{}' via binary search", tags[*index]))
      .filter(|index| !Self::tag_deleted(tags, &mut self.deleted_tags, *index, undelete))
    {
      return Some(index);
    }

    // If we have added any tags, they will be unsorted at the end of the vector. We do a linear
    // search over these. In general we assume that we are not adding many new tags and if we
    // do add them we are not looking them up again. Everything will get sorted when this wrapper
    // is dropped.
    tags[tag_insertion_index ..]
      .iter()
      .position(|t| t.tag == tag_name)
      .map(|index| index + tag_insertion_index)
      .inspect(|index| log::trace!("found tag '{}' via linear search", tags[*index]))
      .filter(|index| !Self::tag_deleted(tags, &mut self.deleted_tags, *index, undelete))
  }

  pub fn change_name(&mut self, name: Bytes) {
    self.metric.metric.id.name = name;
    self.name_changed = true;
  }

  #[must_use]
  pub fn metric(&self) -> &ParsedMetric {
    self.metric
  }
}

impl Drop for EditableParsedMetric<'_> {
  fn drop(&mut self) {
    // If we have done deletion we have to actually go through and do the deletion given the deleted
    // indexes. This is tricky as we do this in place before resorting.
    if let Some(deleted_tags) = &self.deleted_tags {
      debug_assert_eq!(deleted_tags.len(), self.metric.metric.id.tags.len());

      // Swap remove deletion must be handled in reverse order to avoid invalidating indexes.
      for (index, deleted) in deleted_tags.iter().enumerate().rev() {
        if *deleted {
          log::trace!(
            "tag {index}/'{}' deleted",
            self.metric.metric.id.tags[index]
          );
          self.metric.metric.id.tags.swap_remove(index);
        }
      }
    }

    if self.tag_insertion_index.is_some() || self.deleted_tags.is_some() {
      self.metric.metric.id.tags.sort_unstable();
    }

    if self.tag_insertion_index.is_some() || self.name_changed || self.deleted_tags.is_some() {
      self.metric.cached_metric = CachedMetric::NotInitialized;
    }
  }
}

//
// ParsedMetric
//

// A received metric that was successfully parsed.
#[derive(Clone, Debug)]
pub struct ParsedMetric {
  metric: Metric,
  source: MetricSource,
  received_at: Instant,
  cached_metric: CachedMetric,
  downstream_id: DownstreamId,
  metadata: Option<Arc<Metadata>>,
}

impl PartialEq for ParsedMetric {
  fn eq(&self, other: &Self) -> bool {
    // TODO(mattklein123): Add equality for source and downstream_id and fix relevant tests.
    self.metric == other.metric && self.metadata == other.metadata
  }
}

impl ParsedMetric {
  pub fn cached_metric(&self) -> CachedMetric {
    self.cached_metric.clone()
  }

  pub const fn metric(&self) -> &Metric {
    &self.metric
  }

  pub fn into_metric(self) -> (Metric, MetricSource) {
    (self.metric, self.source)
  }

  pub const fn received_at(&self) -> Instant {
    self.received_at
  }

  pub const fn source(&self) -> &MetricSource {
    &self.source
  }

  pub const fn downstream_id(&self) -> &DownstreamId {
    &self.downstream_id
  }

  pub const fn metadata(&self) -> Option<&Arc<Metadata>> {
    self.metadata.as_ref()
  }

  pub fn set_metadata(&mut self, metadata: Option<Arc<Metadata>>) {
    self.metadata = metadata;
  }

  pub const fn new(
    metric: Metric,
    source: MetricSource,
    received_at: Instant,
    downstream_id: DownstreamId,
  ) -> Self {
    Self {
      metric,
      source,
      received_at,
      cached_metric: CachedMetric::NotInitialized,
      downstream_id,
      metadata: None,
    }
  }

  pub fn from_write_request(
    write_request: WriteRequest,
    received_at: Instant,
    parse_config: &ParseConfig,
    downstream_id_provider: &dyn DownstreamIdProvider,
  ) -> (Vec<Self>, Vec<ParseError>) {
    let result = from_write_request(write_request, parse_config);
    (
      result
        .0
        .into_iter()
        .map(|metric| {
          let downstream_id = downstream_id_provider.downstream_id(metric.get_id());
          Self::new(
            metric,
            MetricSource::PromRemoteWrite,
            received_at,
            downstream_id,
          )
        })
        .collect(),
      result.1,
    )
  }

  #[must_use]
  pub fn to_write_request(
    parsed_metrics: Vec<Self>,
    options: &ToWriteRequestOptions,
    changed_type_tracker: &ChangedTypeTracker,
  ) -> WriteRequest {
    to_write_request(parsed_metrics, options, changed_type_tracker)
  }

  pub fn try_from_wire_protocol(
    original: bytes::Bytes,
    wire_protocol: &WireProtocol,
    received_at: Instant,
    downstream_id: DownstreamId,
  ) -> Result<Self, ParseError> {
    let res = match &wire_protocol.protocol_type {
      Some(Protocol_type::Carbon(_)) => Self::new(
        crate::protos::carbon::parse(&original)?,
        MetricSource::Carbon(original),
        received_at,
        downstream_id,
      ),
      Some(Protocol_type::Statsd(statsd)) => Self::new(
        crate::protos::statsd::parse(&original, statsd)?,
        MetricSource::Statsd(original),
        received_at,
        downstream_id,
      ),
      None => unreachable!("pgv"),
    };
    Ok(res)
  }

  pub fn to_wire_protocol(&self, protocol: &WireProtocol) -> bytes::Bytes {
    match (&protocol.protocol_type, &self.source) {
      (Some(Protocol_type::Carbon(_)), MetricSource::Carbon(original))
      | (Some(Protocol_type::Statsd(_)), MetricSource::Statsd(original)) => original.clone(),
      (Some(Protocol_type::Carbon(_)), _) => to_carbon_line(&self.metric),
      (Some(Protocol_type::Statsd(_)), _) => to_statsd_line(&self.metric),
      (None, _) => unreachable!("pgv"),
    }
  }

  pub fn initialize_cache(&mut self, metric_cache: &Arc<MetricCache>) {
    if matches!(self.cached_metric, CachedMetric::NotInitialized) {
      self.cached_metric = metric_cache.get(&self.metric, self.received_at);
    }
  }
}
