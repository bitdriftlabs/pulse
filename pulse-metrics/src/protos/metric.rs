// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/licenses/strict/1.0.0.txt

#[cfg(test)]
#[path = "./metric_test.rs"]
mod metric_test;

use super::carbon::to_carbon_line;
use super::prom::{
  ChangedTypeTracker,
  ToWriteRequestOptions,
  from_write_request,
  to_write_request,
};
use super::statsd::to_statsd_line;
use crate::pipeline::metric_cache::{CachedMetric, MetricCache};
pub use bd_otlp_metrics::{
  CounterType,
  HistogramBucket,
  HistogramData,
  Metric,
  MetricId,
  MetricType,
  MetricValue,
  ParseError,
  SummaryBucket,
  SummaryData,
  TagValue,
  default_timestamp,
  unwrap_timestamp,
};
use bd_proto::protos::prometheus::prompb::remote::WriteRequest;
use bytes::Bytes;
use config::common::v1::common::WireProtocol;
use config::common::v1::common::wire_protocol::Protocol_type;
use config::inflow::v1::prom_remote_write::prom_remote_write_server_config::ParseConfig;
#[cfg(test)]
pub(crate) use metric_test::arbitraries::{ArbitraryMetric, ArbitraryParsedMetric};
use protobuf::Chars;
use pulse_common::metadata::Metadata;
use pulse_protobuf::protos::pulse::config;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Instant;

fn tags_sorted(tags: &[TagValue]) -> bool {
  let mut sorted_tags = tags.to_vec();
  sorted_tags.sort_unstable();
  tags == sorted_tags
}

pub fn metric_to_wire_format(metric: &Metric, wire_protocol: &WireProtocol) -> bytes::Bytes {
  match wire_protocol.protocol_type {
    Some(Protocol_type::Statsd(_)) => to_statsd_line(metric),
    Some(Protocol_type::Carbon(_)) => to_carbon_line(metric),
    None => unreachable!("pgv"),
  }
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

  /// Returns the length of the prefix representation without allocating.
  fn prefix_len(&self) -> usize {
    match self {
      Self::LocalOrigin => 5, // "local"
      Self::IpAddress(addr) => {
        // "ip:" + address string length
        3 + match addr {
          std::net::IpAddr::V4(_) => {
            // Max IPv4: "255.255.255.255" = 15 chars
            15
          },
          std::net::IpAddr::V6(_) => {
            // Max IPv6: 39 chars (8 groups of 4 hex digits + 7 colons)
            39
          },
        }
      },
      Self::UnixDomainSocket(name) => 4 + name.len(), // "uds:" + name
      Self::InflowProvided(data) => data.len(),
    }
  }

  /// Writes the prefix representation to the given buffer. Returns the number of bytes written.
  fn write_prefix_to(&self, buf: &mut [u8]) -> usize {
    match self {
      Self::LocalOrigin => {
        buf[.. 5].copy_from_slice(b"local");
        5
      },
      Self::IpAddress(addr) => {
        buf[.. 3].copy_from_slice(b"ip:");
        // Use itoa-style writing for the IP address to avoid allocation
        let addr_str = addr.to_string();
        let addr_bytes = addr_str.as_bytes();
        buf[3 .. 3 + addr_bytes.len()].copy_from_slice(addr_bytes);
        3 + addr_bytes.len()
      },
      Self::UnixDomainSocket(name) => {
        buf[.. 4].copy_from_slice(b"uds:");
        let name_bytes = name.as_bytes();
        buf[4 .. 4 + name_bytes.len()].copy_from_slice(name_bytes);
        4 + name_bytes.len()
      },
      Self::InflowProvided(data) => {
        buf[.. data.len()].copy_from_slice(data);
        data.len()
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
  mtype_changed: bool,
  deleted_tags: Option<Vec<bool>>,
}

impl<'a> EditableParsedMetric<'a> {
  pub fn new(metric: &'a mut ParsedMetric) -> Self {
    Self {
      metric,
      tag_insertion_index: None,
      name_changed: false,
      mtype_changed: false,
      deleted_tags: None,
    }
  }

  pub fn assign_tags(&mut self, mut tags: Vec<TagValue>) {
    // Just sort now so we effectively do a full reset. In the common case scripts are not going
    // to do this and then do more edits.
    tags.sort_unstable();
    *self.metric.metric.get_id_mut().tags_mut() = tags;
    self.tag_insertion_index = None;
    self.deleted_tags = None;
  }

  pub fn add_or_change_tag(&mut self, tag: TagValue) {
    log::trace!("adding or changing tag: {tag}");
    if let Some(existing_tag) = self
      .find_tag_inner(&tag.tag, true)
      .map(|index| &mut self.metric.metric.get_id_mut().tags_mut()[index])
    {
      existing_tag.value = tag.value;
    } else {
      self.metric.metric.get_id_mut().tags_mut().push(tag);
      if self.tag_insertion_index.is_none() {
        self.tag_insertion_index = Some(self.metric.metric.get_id().tags().len() - 1);
      }
      if let Some(deleted_tags) = &mut self.deleted_tags {
        deleted_tags.push(false);
      }
    }
  }

  pub fn find_tag(&mut self, tag_name: &[u8]) -> Option<&mut TagValue> {
    self
      .find_tag_inner(tag_name, false)
      .map(|index| &mut self.metric.metric.get_id_mut().tags_mut()[index])
  }

  pub fn delete_tag(&mut self, tag_name: &[u8]) -> Option<Bytes> {
    if let Some(index) = self.find_tag_inner(tag_name, false) {
      let deleted_tags = self
        .deleted_tags
        .get_or_insert_with(|| vec![false; self.metric.metric.get_id().tags().len()]);
      deleted_tags[index] = true;
      log::trace!(
        "tag '{}' marked for deletion",
        self.metric.metric.get_id().tags()[index]
      );
      Some(self.metric.metric.get_id().tags()[index].value.clone())
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
    let tags = self.metric.metric.get_id().tags();
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
    self.metric.metric.get_id_mut().set_name(name);
    self.name_changed = true;
  }

  pub fn change_mtype(&mut self, mtype: &[u8]) -> Result<(), &'static str> {
    if !matches!(self.metric.metric.value, MetricValue::Simple(_)) {
      return Err("assigning to mtype is only supported for simple metrics");
    }

    let mtype = match mtype {
      b"counter" => MetricType::Counter(CounterType::Absolute),
      b"gauge" => MetricType::Gauge,
      b"delta_gauge" => MetricType::DeltaGauge,
      b"direct_gauge" => MetricType::DirectGauge,
      b"timer" => MetricType::Timer,
      _ => return Err("assigning to mtype requires a supported metric type string"),
    };

    if self.metric.metric.get_id().mtype() != Some(mtype) {
      self.metric.metric.get_id_mut().set_mtype(mtype);
      self.mtype_changed = true;
    }

    Ok(())
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
      debug_assert_eq!(deleted_tags.len(), self.metric.metric.get_id().tags().len());

      // Swap remove deletion must be handled in reverse order to avoid invalidating indexes.
      for (index, deleted) in deleted_tags.iter().enumerate().rev() {
        if *deleted {
          log::trace!(
            "tag {index}/'{}' deleted",
            self.metric.metric.get_id().tags()[index]
          );
          self
            .metric
            .metric
            .get_id_mut()
            .tags_mut()
            .swap_remove(index);
        }
      }
    }

    if self.tag_insertion_index.is_some() || self.deleted_tags.is_some() {
      self.metric.metric.get_id_mut().tags_mut().sort_unstable();
    }

    if self.tag_insertion_index.is_some()
      || self.name_changed
      || self.mtype_changed
      || self.deleted_tags.is_some()
    {
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

  /// Appends a suffix to the downstream ID, converting to InflowProvided if necessary.
  pub fn append_to_downstream_id(&mut self, suffix: &[u8]) {
    // Calculate total size: prefix + ':' + suffix
    let prefix_max_len = self.downstream_id.prefix_len();
    let total_capacity = prefix_max_len + 1 + suffix.len();

    // Allocate once with exact capacity
    let mut buf = bytes::BytesMut::with_capacity(total_capacity);

    // SAFETY: We've allocated enough capacity, and we'll set the length after writing
    // Use the spare capacity to write prefix directly
    unsafe {
      let spare = buf.spare_capacity_mut();
      let spare_ptr = spare.as_mut_ptr().cast::<u8>();
      let spare_slice = std::slice::from_raw_parts_mut(spare_ptr, total_capacity);

      let prefix_len = self.downstream_id.write_prefix_to(spare_slice);
      spare_slice[prefix_len] = b':';
      spare_slice[prefix_len + 1 .. prefix_len + 1 + suffix.len()].copy_from_slice(suffix);

      buf.set_len(prefix_len + 1 + suffix.len());
    }

    self.downstream_id = DownstreamId::InflowProvided(buf.freeze());
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
