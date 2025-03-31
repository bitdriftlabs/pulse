// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./metric_cache_test.rs"]
mod metric_cache_test;

use crate::lru_map::{Equivalent, LruMap};
use crate::protos::metric::{CounterType, Metric, MetricId, MetricType, TagValue};
use bd_server_stats::stats::Scope;
use bytes::{Buf, BufMut, Bytes};
use parking_lot::Mutex;
use prometheus::{IntCounter, IntGauge};
use std::any::Any;
use std::hash::Hash;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

//
// StateSlotHandle
//

// A handle for an individual state slot inside a cached metric. This allows different components
// to store arbitrary state for a given metric efficiently.
pub struct StateSlotHandle {
  id: usize,
  index: usize,
  free_slot_list: Arc<Mutex<Vec<usize>>>,
}

impl Drop for StateSlotHandle {
  fn drop(&mut self) {
    log::debug!("release slot handle id={} index={}", self.id, self.index);
    self.free_slot_list.lock().push(self.index);
  }
}

//
// Stats
//

// State for the cache.
#[derive(Clone, Debug)]
struct Stats {
  cache_size: IntGauge,
  overflow: IntCounter,
}

impl Stats {
  pub fn new(scope: &Scope) -> Self {
    let scope = scope.scope("metric_cache");
    Self {
      cache_size: scope.gauge("cache_size"),
      overflow: scope.counter("overflow"),
    }
  }
}

//
// CachedMetric
//

// Wraps a cached metric, which can either be not initialized, loaded, or overflow, if there is not
// currently space to cache the metric. Each component has to decide what to do in the overflow
// state.
#[derive(Clone, Debug)]
pub enum CachedMetric {
  NotInitialized,
  Loaded(Arc<MetricKey>, Arc<StateSlots>),
  Overflow,
}

impl CachedMetric {
  #[cfg(test)]
  #[must_use]
  pub fn state_slots(&self) -> Arc<StateSlots> {
    match self {
      Self::Loaded(_, state_slots) => state_slots.clone(),
      Self::NotInitialized | Self::Overflow => unreachable!(),
    }
  }
}

//
// GetOrInitResult
//

// Return type from get_or_init() below.
#[derive(Debug)]
pub enum GetOrInitResult<T> {
  Inserted(Arc<T>),
  Existed(Arc<T>),
}

//
// StateSlots
//

// Wraps arbitrary data storage for each cached metric.
#[derive(Debug, Clone)]
struct StateSlotEntry {
  id: usize,
  slot: Arc<dyn Any + Send + Sync>,
}
#[derive(Debug)]
pub struct StateSlots {
  slots: Mutex<Vec<Option<StateSlotEntry>>>,
}

impl StateSlots {
  // Create a new slots structure with the expected size.
  fn new(default_size: usize) -> Self {
    Self {
      slots: Mutex::new(vec![None; default_size]),
    }
  }

  // Get a slot value that may or may not have been initialized.
  pub fn get<T: Send + Sync + 'static>(&self, handle: &StateSlotHandle) -> Option<Arc<T>> {
    self
      .slots
      .lock()
      .get(handle.index)
      .and_then(Option::as_ref)
      .and_then(|slot| {
        if slot.id == handle.id {
          Some(slot.slot.clone().downcast().unwrap())
        } else {
          None
        }
      })
  }

  // Get a slot value or initialize it via a passed initialization function if it does not exist.
  pub fn get_or_init<T: Send + Sync + 'static>(
    &self,
    handle: &StateSlotHandle,
    init: impl FnOnce() -> T,
  ) -> GetOrInitResult<T> {
    let mut slots = self.slots.lock();
    // Make sure the slots vector is the right size.
    if slots.len() < handle.index + 1 {
      slots.resize(handle.index + 1, None);
    }

    // If the slot already has data and is the right id, downcast and return it. Otherwise, make a
    // new value, insert it, and return it.
    if let Some(entry) = &slots[handle.index] {
      if entry.id == handle.id {
        return GetOrInitResult::Existed(entry.slot.clone().downcast().unwrap());
      }
    }

    let entry = Arc::new(init());
    slots[handle.index] = Some(StateSlotEntry {
      id: handle.id,
      slot: entry.clone(),
    });
    GetOrInitResult::Inserted(entry)
  }
}

//
// CachedMetricState
//

// State for each cached metric.
struct CachedMetricState {
  last_seen: Instant,
  state_slots: Arc<StateSlots>,
}

//
// MetricKey
//

// This is the key that is stored in the cache LRU map. We *cannot* directly store the MetricID as
// for wire protocols, the data in the MetricID are Byte references into the entire incoming
// payload. Thus, for map insertion we convert to completely owned data. However, we allow lookup
// via the MetricID so in the common cached case we won't have to allocate anything. Because the
// metrics cache makes up a very large portion of the RAM an aggregation server will utilize, it
// uses a compact representation which should still be reasonably quick to hash.
#[derive(Debug, PartialEq, Eq)]
pub struct MetricKey {
  data: Vec<u8>,
}

impl MetricKey {
  const fn metric_type_to_u8(mtype: Option<MetricType>) -> u8 {
    match mtype {
      None => 0,
      Some(MetricType::Counter(CounterType::Delta)) => 1,
      Some(MetricType::Counter(CounterType::Absolute)) => 2,
      Some(MetricType::DeltaGauge) => 3,
      Some(MetricType::DirectGauge) => 4,
      Some(MetricType::Gauge) => 5,
      Some(MetricType::Histogram) => 6,
      Some(MetricType::Summary) => 7,
      Some(MetricType::Timer) => 8,
      Some(MetricType::BulkTimer) => 9,
    }
  }

  fn u8_to_metric_type(mtype: u8) -> Option<MetricType> {
    match mtype {
      0 => None,
      1 => Some(MetricType::Counter(CounterType::Delta)),
      2 => Some(MetricType::Counter(CounterType::Absolute)),
      3 => Some(MetricType::DeltaGauge),
      4 => Some(MetricType::DirectGauge),
      5 => Some(MetricType::Gauge),
      6 => Some(MetricType::Histogram),
      7 => Some(MetricType::Summary),
      8 => Some(MetricType::Timer),
      9 => Some(MetricType::BulkTimer),
      _ => unreachable!(),
    }
  }

  // Create a packed metric key from a metric ID.
  pub fn new(metric_id: &MetricId) -> Self {
    // TODO(mattklein123): If we wanted to be even more compact we could use a varint of some type.

    // Compute the exact length to reduce the number of underlying allocations.
    // 2 bytes for the number of tags, followed by the tags.
    let tag_len: usize = 2
      + metric_id
      .tags()
      .iter()
      .map(|t| t.tag.len() + t.value.len() + 4) // 2 byte length for each.
      .sum::<usize>();
    // 2 bytes for the name length, the name, 1 byte for the type, followed by the tags.
    let len = 2 + metric_id.name().len() + 1 + tag_len;

    let mut data = Vec::with_capacity(len);
    data.put_u16(metric_id.name().len() as u16);
    data.put(metric_id.name().as_ref());
    data.put_u8(Self::metric_type_to_u8(metric_id.mtype()));
    data.put_u16(metric_id.tags().len() as u16);
    for tag in metric_id.tags() {
      data.put_u16(tag.tag.len() as u16);
      data.put(tag.tag.as_ref());
      data.put_u16(tag.value.len() as u16);
      data.put(tag.value.as_ref());
    }
    Self { data }
  }

  // Return a reference to the name within the packed structure.
  #[must_use]
  pub fn name(&self) -> &[u8] {
    let mut buf = &self.data[..];
    let name_len = buf.get_u16();
    &self.data[2 .. 2 + name_len as usize]
  }

  // Convert the packed structure backed to a metric ID. Useful when going from storage back to
  // other use cases.
  #[must_use]
  pub fn to_metric_id(&self) -> MetricId {
    let mut buf = &self.data[..];
    let name_len = buf.get_u16() as usize;
    let name = Bytes::copy_from_slice(&buf.chunk()[.. name_len]);
    buf.advance(name_len);

    let metric_type = Self::u8_to_metric_type(buf.get_u8());
    let tags_len = buf.get_u16() as usize;
    let mut tags = Vec::with_capacity(tags_len);
    for _ in 0 .. tags_len {
      let tag_name_len = buf.get_u16() as usize;
      let tag = Bytes::copy_from_slice(&buf.chunk()[.. tag_name_len]);
      buf.advance(tag_name_len);

      let tag_value_len = buf.get_u16() as usize;
      let value = Bytes::copy_from_slice(&buf.chunk()[.. tag_value_len]);
      buf.advance(tag_value_len);
      tags.push(TagValue { tag, value });
    }

    MetricId::new(name, metric_type, tags, true).unwrap()
  }
}

impl Hash for MetricKey {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    let mut buf = &self.data[..];
    let name_len = buf.get_u16() as usize;
    buf.chunk()[.. name_len].hash(state);
    buf.advance(name_len);

    Self::u8_to_metric_type(buf.get_u8()).hash(state);

    let tags_len = buf.get_u16() as usize;
    for _ in 0 .. tags_len {
      let tag_name_len = buf.get_u16() as usize;
      buf.chunk()[.. tag_name_len].hash(state);
      buf.advance(tag_name_len);

      let tag_value_len = buf.get_u16() as usize;
      buf.chunk()[.. tag_value_len].hash(state);
      buf.advance(tag_value_len);
    }
  }
}

impl PartialEq<MetricId> for MetricKey {
  fn eq(&self, other: &MetricId) -> bool {
    let mut buf = &self.data[..];
    let name_len = buf.get_u16() as usize;
    let mut index = 2;

    // Get name and advance past name bytes.
    let name = &self.data[index .. index + name_len];
    buf.advance(name_len);
    index += name_len;

    // Get metric type and the number of tags and advance past those bytes.
    let mtype = Self::u8_to_metric_type(buf.get_u8());
    let tags_len = buf.get_u16() as usize;
    index += 3;

    if tags_len != other.tags().len() {
      return false;
    }
    for other_tag in other.tags() {
      let tag_name_len = buf.get_u16() as usize;
      index += 2;
      let tag_name = &self.data[index .. index + tag_name_len];
      buf.advance(tag_name_len);
      index += tag_name_len;

      let tag_value_len = buf.get_u16() as usize;
      index += 2;
      let tag_value = &self.data[index .. index + tag_value_len];
      buf.advance(tag_value_len);
      index += tag_value_len;

      if tag_name != other_tag.tag || tag_value != other_tag.value {
        return false;
      }
    }

    name == other.name() && mtype == other.mtype()
  }
}

impl Equivalent<MetricId> for Arc<MetricKey> {
  fn equivalent(&self, other: &MetricId) -> bool {
    *self.as_ref() == *other
  }
}

impl Equivalent<Self> for Arc<MetricKey> {
  fn equivalent(&self, other: &Self) -> bool {
    self == other
  }
}

//
// MetricCache
//

// The metric cache provides an efficient LRU based cache for metrics, along with arbitrary per
// component state. Caching and eviction is handled centrally, and as long as there is sufficient
// memory, state will be stored indefinitely for components such as sampling, elision, and
// aggregation.
pub struct MetricCache {
  cache: LruMap<Arc<MetricKey>, CachedMetricState, ahash::RandomState>,
  next_id: AtomicUsize,
  next_state_slot: AtomicUsize,
  free_slot_list: Arc<Mutex<Vec<usize>>>,
  stats: Stats,
  max_cached_metrics: Option<u64>,
}

impl MetricCache {
  #[must_use]
  pub fn new(scope: &Scope, max_cached_metrics: Option<u64>) -> Arc<Self> {
    Arc::new(Self {
      cache: LruMap::with_hasher(ahash::RandomState::default()),
      next_id: AtomicUsize::default(),
      next_state_slot: AtomicUsize::default(),
      free_slot_list: Arc::default(),
      stats: Stats::new(scope),
      max_cached_metrics,
    })
  }

  #[must_use]
  #[cfg(test)]
  fn new_single_shard(scope: &Scope) -> Arc<Self> {
    Arc::new(Self {
      cache: LruMap::with_hasher_and_shards(ahash::RandomState::default(), 1),
      next_id: AtomicUsize::default(),
      next_state_slot: AtomicUsize::default(),
      free_slot_list: Arc::default(),
      stats: Stats::new(scope),
      max_cached_metrics: None,
    })
  }

  pub fn state_slot_handle(&self) -> StateSlotHandle {
    let index = self
      .free_slot_list
      .lock()
      .pop()
      .unwrap_or_else(|| self.next_state_slot.fetch_add(1, Ordering::SeqCst));
    let id = self.next_id.fetch_add(1, Ordering::SeqCst);
    log::debug!("new slot handle id={id} index={index}");
    StateSlotHandle {
      id,
      index,
      free_slot_list: self.free_slot_list.clone(),
    }
  }

  pub fn get(self: &Arc<Self>, metric: &Metric, received_at: Instant) -> CachedMetric {
    // TODO(mattklein123): Make this configurable.
    const MAX_AGE: Duration = Duration::from_secs(60 * 60);

    // First, try to get the metric without obtaining a write lock.
    if let Some(cached_metric) = self.cache.get(metric.get_id()) {
      if received_at - cached_metric.value().last_seen < MAX_AGE {
        log::trace!("returning cached metric: {}", metric.get_id());
        return CachedMetric::Loaded(
          cached_metric.key().clone(),
          cached_metric.value().state_slots.clone(),
        );
      }
    }

    // Check to see if have overflowed. If so, attempt to purge, and if that fails, return the
    // overflowed status.
    // TODO(mattklein123): Technically we should only do this when we are about to insert since
    // this can also trigger when the eviction time has elapsed. That will require changing the
    // LRU map a bit and seems not worth it right now.
    if self
      .max_cached_metrics
      .is_some_and(|max| self.cache.len() >= max as usize)
      && self
        .cache
        .try_evict_lru(|_, value| received_at - value.last_seen >= MAX_AGE)
        == 0
    {
      log::debug!("not creating key due to overflow");
      self.stats.overflow.inc();
      return CachedMetric::Overflow;
    }

    // TODO(mattklein123): We can avoid both the Arc::new() as well as the MetricKey construction
    // in the case where the key already exists and we are refreshing it.
    let result = self.cache.get_or_insert_map(
      Arc::new(MetricKey::new(metric.get_id())),
      || CachedMetricState {
        last_seen: received_at,
        state_slots: Arc::new(StateSlots::new(
          self.next_state_slot.load(Ordering::Relaxed),
        )),
      },
      |key, value| {
        log::debug!(
          "updating last seen and returning cached metric: {}",
          metric.get_id()
        );
        value.last_seen = received_at;
        CachedMetric::Loaded(key.clone(), value.state_slots.clone())
      },
      |key, value| {
        log::debug!("creating new cached metric: {}", metric.get_id());
        CachedMetric::Loaded(key.clone(), value.state_slots.clone())
      },
      |_, value| received_at - value.last_seen >= MAX_AGE,
    );
    self
      .stats
      .cache_size
      .set(self.cache.len().try_into().unwrap());
    result
  }

  pub fn iterate(&self, mut iterate_func: impl FnMut(&MetricKey, &StateSlots)) {
    self
      .cache
      .iterate(|key, value| iterate_func(key, &value.state_slots));
  }
}
