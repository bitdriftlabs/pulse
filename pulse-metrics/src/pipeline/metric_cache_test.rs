// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::{MetricCache, MetricKey};
use crate::lru_map::Equivalent;
use crate::pipeline::metric_cache::{CachedMetric, GetOrInitResult};
use crate::protos::metric::{
  DownstreamId,
  Metric,
  MetricId,
  MetricSource,
  MetricType,
  MetricValue,
  ParsedMetric,
};
use crate::test::make_metric_id;
use bd_server_stats::stats::Collector;
use matches::assert_matches;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn make_metric(name: &str, metric_cache: &Arc<MetricCache>, received_at: Instant) -> ParsedMetric {
  let mut parsed_metric = ParsedMetric::new(
    Metric::new(
      MetricId::new(name.to_string().into(), None, vec![], false).unwrap(),
      None,
      2,
      MetricValue::Simple(1.),
    ),
    MetricSource::PromRemoteWrite,
    received_at,
    DownstreamId::LocalOrigin,
  );
  parsed_metric.initialize_cache(metric_cache);
  parsed_metric
}

#[test]
fn reuse_slots() {
  let metric_cache = MetricCache::new_single_shard(&Collector::default().scope("test"));
  let handle1 = metric_cache.state_slot_handle();
  let handle2 = metric_cache.state_slot_handle();
  let now = Instant::now();
  let metric1 = make_metric("metric1", &metric_cache, now);

  assert_matches!(
    metric1
      .cached_metric()
      .state_slots()
      .get_or_init(&handle1, || 100_u64),
    GetOrInitResult::Inserted(_)
  );

  drop(handle1);
  drop(handle2);
  let handle1 = metric_cache.state_slot_handle();
  let handle2 = metric_cache.state_slot_handle();

  // Even though index 0 previously had data it should not now since the id has changed.
  assert!(metric1
    .cached_metric()
    .state_slots()
    .get::<u64>(&handle1)
    .is_none());
  assert!(metric1
    .cached_metric()
    .state_slots()
    .get::<u64>(&handle2)
    .is_none());

  // Even though index 0 previously had data it should reinitialize since the id has changed.
  assert_matches!(
    metric1
      .cached_metric()
      .state_slots()
      .get_or_init(&handle2, || 100_u64),
    GetOrInitResult::Inserted(_)
  );
}

#[test]
fn slots() {
  let metric_cache = MetricCache::new_single_shard(&Collector::default().scope("test"));
  let handle1 = metric_cache.state_slot_handle();
  let handle2 = metric_cache.state_slot_handle();
  let now = Instant::now();
  let metric1 = make_metric("metric1", &metric_cache, now);

  assert!(metric1
    .cached_metric()
    .state_slots()
    .get::<u64>(&handle1)
    .is_none());
  assert!(metric1
    .cached_metric()
    .state_slots()
    .get::<u64>(&handle2)
    .is_none());

  assert_matches!(
    metric1
      .cached_metric()
      .state_slots()
      .get_or_init(&handle1, || 100_u64),
    GetOrInitResult::Inserted(_)
  );
  assert_matches!(
    metric1
      .cached_metric()
      .state_slots()
      .get_or_init(&handle2, || "hello"),
    GetOrInitResult::Inserted(_)
  );
  assert_eq!(
    100,
    *metric1
      .cached_metric()
      .state_slots()
      .get::<u64>(&handle1)
      .unwrap()
  );
  assert_matches!(
    metric1
      .cached_metric()
      .state_slots()
      .get_or_init(&handle1, || 200_u64), GetOrInitResult::Existed(existed) if *existed == 100
  );
  assert_eq!(
    "hello",
    *metric1
      .cached_metric()
      .state_slots()
      .get::<&str>(&handle2)
      .unwrap()
  );
  assert_matches!(
    metric1
      .cached_metric()
      .state_slots()
      .get_or_init(&handle2, || "hello2"), GetOrInitResult::Existed(existed) if *existed == "hello"
  );

  // The retention is currently hard coded to 1 hour, make sure that if we advance past that, but
  // with the same metric, we retain the previous metric and bump the time.
  let metric1 = make_metric("metric1", &metric_cache, now + Duration::from_secs(61 * 60));
  assert_matches!(
    metric1
      .cached_metric()
      .state_slots()
      .get_or_init(&handle2, || "hello2"), GetOrInitResult::Existed(existed) if *existed == "hello"
  );

  // Advance further with a new metric, this should LRU metric1.
  let metric2 = make_metric(
    "metric2",
    &metric_cache,
    now + Duration::from_secs(122 * 60),
  );
  assert_matches!(
    metric2
      .cached_metric()
      .state_slots()
      .get_or_init(&handle1, || 1000_u64),
    GetOrInitResult::Inserted(_)
  );
  let metric1 = make_metric(
    "metric1",
    &metric_cache,
    now + Duration::from_secs(122 * 60),
  );
  assert_matches!(
    metric1
      .cached_metric()
      .state_slots()
      .get_or_init(&handle2, || "world"),
    GetOrInitResult::Inserted(_)
  );
}

#[test]
fn metric_cache_overflow() {
  let metric_cache = MetricCache::new(&Collector::default().scope("test"), Some(1));
  let now = Instant::now();

  let metric1 = make_metric("metric1", &metric_cache, now);
  assert_matches!(metric1.cached_metric(), CachedMetric::Loaded(_, _));
  let metric2 = make_metric("metric2", &metric_cache, now);
  assert_matches!(metric2.cached_metric(), CachedMetric::Overflow);

  let metric2 = make_metric("metric2", &metric_cache, now + Duration::from_secs(61 * 60));
  assert_matches!(metric2.cached_metric(), CachedMetric::Loaded(_, _));
}

#[test]
fn metric_cache_dedup_keys() {
  let metric_cache = MetricCache::new(&Collector::default().scope("test"), None);
  let now = Instant::now();

  let metric = make_metric("my_metric", &metric_cache, now);
  let metric_clone = make_metric("my_metric", &metric_cache, now);

  assert!(Arc::ptr_eq(
    &metric.cached_metric().state_slots(),
    &metric_clone.cached_metric().state_slots()
  ));
}

fn test_equivalent(metric_id: &MetricId, metric_key: &Arc<MetricKey>, expect_equal: bool) {
  let hash_builder = ahash::RandomState::default();
  assert_eq!(expect_equal, metric_key.equivalent(metric_id));
  let metric_id_hash = hash_builder.hash_one(metric_id);
  let metric_key_hash = hash_builder.hash_one(metric_key);
  assert_eq!(expect_equal, metric_id_hash == metric_key_hash);
}

#[test]
fn hash_equivalent() {
  test_equivalent(
    &make_metric_id("foo", None, &[]),
    &Arc::new(MetricKey::new(&make_metric_id("foo", None, &[]))),
    true,
  );
  test_equivalent(
    &make_metric_id("foo", Some(MetricType::Histogram), &[]),
    &Arc::new(MetricKey::new(&make_metric_id(
      "foo",
      Some(MetricType::Histogram),
      &[],
    ))),
    true,
  );
  test_equivalent(
    &make_metric_id("foo", Some(MetricType::Histogram), &[("foo", "bar")]),
    &Arc::new(MetricKey::new(&make_metric_id(
      "foo",
      Some(MetricType::Histogram),
      &[("foo", "bar")],
    ))),
    true,
  );
  test_equivalent(
    &make_metric_id("foo", Some(MetricType::Histogram), &[("foo", "bar")]),
    &Arc::new(MetricKey::new(&make_metric_id(
      "foo",
      Some(MetricType::Histogram),
      &[],
    ))),
    false,
  );
  test_equivalent(
    &make_metric_id("foo", Some(MetricType::Histogram), &[("foo", "bar")]),
    &Arc::new(MetricKey::new(&make_metric_id(
      "foo",
      Some(MetricType::Histogram),
      &[("foo", "baz")],
    ))),
    false,
  );
}
