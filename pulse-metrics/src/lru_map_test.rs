// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::*;
use crate::pipeline::metric_cache::MetricKey;
use crate::protos::metric::MetricId;
use crate::test::make_metric_id;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

type TestMap = LruMap<Arc<MetricKey>, String, ahash::RandomState>;

fn insert(
  map: &TestMap,
  key: &MetricId,
  value: &str,
  evict: impl Fn(&MetricKey, &mut String) -> bool,
) {
  let mut inserted = false;
  map.get_or_insert_map(
    Arc::new(MetricKey::new(key)),
    || value.to_string(),
    |_, _| unreachable!(),
    |_, _| inserted = true,
    |key, value| evict(key, value),
  );
  assert!(inserted);
}

fn insert_expect_exists(
  map: &TestMap,
  key: &MetricId,
  value: &str,
  evict: impl Fn(&MetricKey, &mut String) -> bool,
) {
  let mut exists = false;
  map.get_or_insert_map(
    Arc::new(MetricKey::new(key)),
    || value.to_string(),
    |_, _| exists = true,
    |_, _| unreachable!(),
    |key, value| evict(key, value),
  );
  assert!(exists);
}

#[test]
fn all_single_shard() {
  let map: TestMap = LruMap::with_hasher_and_shards(ahash::RandomState::new(), 1);
  assert_eq!(0, map.len());
  assert!(map.is_empty());
  assert!(map.get(&make_metric_id("hello", None, &[])).is_none());
  insert(
    &map,
    &make_metric_id("hello", None, &[]),
    "world",
    |_, _| unreachable!(),
  );
  insert(
    &map,
    &make_metric_id("hello2", None, &[]),
    "world2",
    |key, value| {
      assert_eq!(key.name(), b"hello");
      assert_eq!(*value, "world");
      false
    },
  );
  let iteration = AtomicU64::default();
  insert(
    &map,
    &make_metric_id("hello3", None, &[]),
    "world3",
    |key, value| {
      if iteration.load(Ordering::SeqCst) == 0 {
        iteration.fetch_add(1, Ordering::SeqCst);
        assert_eq!(key.name(), b"hello");
        assert_eq!(*value, "world");
        true
      } else {
        assert_eq!(key.name(), b"hello2");
        assert_eq!(*value, "world2");
        false
      }
    },
  );
  assert_eq!(2, map.len());

  insert(
    &map,
    &make_metric_id("hello", None, &[]),
    "world",
    |key, value| {
      assert_eq!(key.name(), b"hello2");
      assert_eq!(*value, "world2");
      false
    },
  );
  assert_eq!(
    "world",
    map
      .get(&make_metric_id("hello", None, &[]))
      .unwrap()
      .value()
  );
  insert_expect_exists(
    &map,
    &make_metric_id("hello", None, &[]),
    "world",
    |key, value| {
      assert_eq!(key.name(), b"hello2");
      assert_eq!(*value, "world2");
      false
    },
  );
  assert_eq!(3, map.len());
}

#[test]
fn multiple_shards() {
  let map = LruMap::with_hasher(ahash::RandomState::new());
  for i in 0 .. 1000 {
    map.get_or_insert_map(
      Arc::new(MetricKey::new(&make_metric_id(&i.to_string(), None, &[]))),
      || i * 2,
      |_, _| unreachable!(),
      |_, _| {},
      |_, _| false,
    );
  }
  assert_eq!(map.len(), 1000);
  for i in 0 .. 1000 {
    assert_eq!(
      *map
        .get(&make_metric_id(&i.to_string(), None, &[]))
        .unwrap()
        .value(),
      i * 2
    );
  }
  map.try_evict_lru(|_, _| true);
  assert!(map.is_empty());
}

#[test]
fn evict_ends_early() {
  let map = LruMap::with_hasher_and_shards(ahash::RandomState::new(), 1);
  for i in 0 .. 1000 {
    map.get_or_insert_map(
      Arc::new(MetricKey::new(&make_metric_id(&i.to_string(), None, &[]))),
      || i * 2,
      |_, _| unreachable!(),
      |_, _| {},
      |_, _| false,
    );
  }
  assert_eq!(map.len(), 1000);
  let num_calls = AtomicU64::new(0);
  map.try_evict_lru(|_, _| {
    assert!(num_calls.load(Ordering::SeqCst) < 3);
    num_calls.fetch_add(1, Ordering::SeqCst);
    num_calls.load(Ordering::SeqCst) != 3
  });
  assert_eq!(998, map.len());
}
