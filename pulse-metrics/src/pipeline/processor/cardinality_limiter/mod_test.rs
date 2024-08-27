// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::pipeline::processor::cardinality_limiter::CardinalityLimiterProcessor;
use crate::pipeline::processor::PipelineProcessor;
use crate::test::{make_metric, make_metric_with_metadata, processor_factory_context_for_test};
use bd_test_helpers::make_mut;
use cardinality_limiter_config::per_pod_limit::Override_limit_location;
use cardinality_limiter_config::{GlobalLimit, Limit_type, PerPodLimit};
use prometheus::labels;
use pulse_common::k8s::pods_info::container::PodsInfo;
use pulse_common::k8s::test::make_pod_info;
use pulse_common::metadata::Metadata;
use pulse_protobuf::protos::pulse::config::processor::v1::cardinality_limiter::{
  cardinality_limiter_config,
  CardinalityLimiterConfig,
};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;
use tokio::task::yield_now;
use vrl::btreemap;
use xxhash_rust::xxh64::Xxh64;

fn make_metadata(pod_name: &str) -> Metadata {
  Metadata::new(
    "default",
    pod_name,
    &BTreeMap::default(),
    &BTreeMap::default(),
    None,
  )
}

#[tokio::test(start_paused = true)]
async fn all_per_pod() {
  let (mut helper, context) = processor_factory_context_for_test();

  let mut pods_info = PodsInfo::default();
  pods_info.insert(make_pod_info(
    "default",
    "pod1",
    &BTreeMap::default(),
    BTreeMap::default(),
    None,
    HashMap::default(),
    "127.0.0.1",
  ));
  helper.k8s_sender.send(pods_info.clone()).unwrap();

  // Because the cuckoo filter is a probabilistic data structure in order for the test to work we
  // have to use a hash that will be consistent so for the test we use xxhash.
  let processor = Arc::new(
    CardinalityLimiterProcessor::new::<Xxh64>(
      &CardinalityLimiterConfig {
        limit_type: Some(Limit_type::PerPodLimit(PerPodLimit {
          default_size_limit: 5,
          override_limit_location: Some(Override_limit_location::VrlProgram(
            "parse_int!(%k8s.pod.annotations.cardinality_limit)".into(),
          )),
          ..Default::default()
        })),
        buckets: 2,
        rotate_after: Some(Duration::from_secs(600).into()).into(),
        ..Default::default()
      },
      context,
    )
    .await
    .unwrap(),
  );
  processor.clone().start().await;

  // Metric with no metadata should get dropped.
  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .returning(|metrics| {
      assert!(metrics.is_empty());
    });
  processor
    .clone()
    .recv_samples(vec![make_metric("metric 0", &[], 0)])
    .await;
  helper
    .stats_helper
    .assert_counter_eq(1, "processor:drop", &labels! {});

  // Add some metrics for the first pod. Make sure one gets dropped.
  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .returning(|metrics| {
      assert_eq!(5, metrics.len());
    });
  processor
    .clone()
    .recv_samples(
      (0 .. 6)
        .map(|i| make_metric_with_metadata(&format!("metric {i}"), &[], 0, make_metadata("pod1")))
        .collect(),
    )
    .await;
  helper
    .stats_helper
    .assert_counter_eq(2, "processor:drop", &labels! {});

  // Add a new pod with an override limit and make sure it gets an independent limit,
  pods_info.insert(make_pod_info(
    "default",
    "pod2",
    &BTreeMap::default(),
    btreemap!("cardinality_limit" => "3"),
    None,
    HashMap::default(),
    "127.0.0.2",
  ));
  helper.k8s_sender.send(pods_info.clone()).unwrap();
  yield_now().await;
  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .returning(|metrics| {
      assert_eq!(3, metrics.len());
    });
  processor
    .clone()
    .recv_samples(
      (0 .. 6)
        .map(|i| make_metric_with_metadata(&format!("metric {i}"), &[], 0, make_metadata("pod2")))
        .collect(),
    )
    .await;
  helper
    .stats_helper
    .assert_counter_eq(5, "processor:drop", &labels! {});

  // Rotate twice to make sure everything is cleared.
  tokio::time::sleep(Duration::from_secs(601)).await;
  tokio::time::sleep(Duration::from_secs(600)).await;

  // Send some new metrics and make sure they get through.
  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .returning(|metrics| {
      assert_eq!(2, metrics.len());
    });
  processor
    .clone()
    .recv_samples(vec![
      make_metric_with_metadata("metric 10", &[], 0, make_metadata("pod1")),
      make_metric_with_metadata("metric 10", &[], 0, make_metadata("pod2")),
    ])
    .await;

  // Remove one of the pods.
  pods_info.remove("default", "pod1");
  helper.k8s_sender.send(pods_info.clone()).unwrap();
  yield_now().await;

  helper.shutdown_trigger.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn all_global() {
  let (mut helper, context) = processor_factory_context_for_test();
  // Because the cuckoo filter is a probabilistic data structure in order for the test to work we
  // have to use a hash that will be consistent so for the test we use xxhash.
  let processor = Arc::new(
    CardinalityLimiterProcessor::new::<Xxh64>(
      &CardinalityLimiterConfig {
        limit_type: Some(Limit_type::GlobalLimit(GlobalLimit {
          size_limit: 5,
          ..Default::default()
        })),
        buckets: 2,
        rotate_after: Some(Duration::from_secs(600).into()).into(),
        ..Default::default()
      },
      context,
    )
    .await
    .unwrap(),
  );
  processor.clone().start().await;
  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .returning(|metrics| {
      assert_eq!(4, metrics.len());
    });
  processor
    .clone()
    .recv_samples(
      (0 .. 4)
        .map(|i| make_metric(&format!("metric {i}"), &[], 0))
        .collect(),
    )
    .await;

  // Rotate out. We should still have 4 in the second bucket.
  tokio::time::sleep(Duration::from_secs(601)).await;

  // We should only get 2 out of this next batch with 1 drop. One existing metric and one new one.
  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .returning(|metrics| {
      assert_eq!(2, metrics.len());
    });
  processor
    .clone()
    .recv_samples(
      (3 .. 6)
        .map(|i| make_metric(&format!("metric {i}"), &[], 0))
        .collect(),
    )
    .await;
  helper
    .stats_helper
    .assert_counter_eq(1, "processor:drop", &labels! {});

  // Should be an immediate drop.
  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .returning(|metrics| {
      assert!(metrics.is_empty());
    });
  processor
    .clone()
    .recv_samples(
      (6 .. 7)
        .map(|i| make_metric(&format!("metric {i}"), &[], 0))
        .collect(),
    )
    .await;
  helper
    .stats_helper
    .assert_counter_eq(2, "processor:drop", &labels! {});

  // Rotate out. We should still have 1 in the second bucket.
  tokio::time::sleep(Duration::from_secs(600)).await;

  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .returning(|metrics| {
      assert_eq!(4, metrics.len());
    });
  processor
    .recv_samples(
      (6 .. 11)
        .map(|i| make_metric(&format!("metric {i}"), &[], 0))
        .collect(),
    )
    .await;
  helper
    .stats_helper
    .assert_counter_eq(3, "processor:drop", &labels! {});
}
