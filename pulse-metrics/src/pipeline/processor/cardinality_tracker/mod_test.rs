// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::admin::test::MockAdmin;
use crate::pipeline::processor::PipelineProcessor;
use crate::pipeline::processor::cardinality_tracker::CardinalityTrackerProcessor;
use crate::test::{make_metric, processor_factory_context_for_test};
use bd_test_helpers::make_mut;
use cardinality_tracker::CardinalityTrackerConfig;
use cardinality_tracker::cardinality_tracker_config::top_k::{Group_by, TagList};
use cardinality_tracker::cardinality_tracker_config::tracking_type::Type;
use cardinality_tracker::cardinality_tracker_config::{Count, TopK, TrackingType};
use http_body_util::BodyExt;
use pretty_assertions::assert_eq;
use pulse_protobuf::protos::pulse::config::processor::v1::cardinality_tracker;
use std::collections::HashMap;
use std::time::Duration;

const CARDINALITY_GAUGE_METRIC_NAME: &str = "processor:cardinality_tracker";

async fn assert_admin(admin: &MockAdmin, expected: &str) {
  let response = std::string::String::from_utf8(
    admin
      .run("/cardinality_tracker")
      .await
      .into_body()
      .collect()
      .await
      .unwrap()
      .to_bytes()
      .to_vec(),
  )
  .unwrap();
  assert_eq!(expected.trim(), response);
}

fn assert_cardinality_gauge(
  helper: &crate::test::ProcessorFactoryContextHelper,
  value: i64,
  labels: &[(&str, &str)],
) {
  let labels = labels.iter().copied().collect::<HashMap<_, _>>();
  helper
    .stats_helper
    .find_gauge(CARDINALITY_GAUGE_METRIC_NAME, &labels)
    .unwrap_or_else(|| {
      panic!(
        "gauge not found for labels {labels:?}. {:?}",
        helper.stats_helper.collector().debug_output()
      )
    });
  helper
    .stats_helper
    .assert_gauge_eq(value, CARDINALITY_GAUGE_METRIC_NAME, &labels);
}

fn assert_cardinality_gauge_absent(
  helper: &crate::test::ProcessorFactoryContextHelper,
  labels: &[(&str, &str)],
) {
  let labels = labels.iter().copied().collect::<HashMap<_, _>>();
  assert!(
    helper
      .stats_helper
      .find_gauge(CARDINALITY_GAUGE_METRIC_NAME, &labels)
      .is_none()
  );
}

#[tokio::test(start_paused = true)]
async fn all() {
  let (mut helper, context) = processor_factory_context_for_test();
  let processor = CardinalityTrackerProcessor::new(
    CardinalityTrackerConfig {
      tracking_types: vec![
        TrackingType {
          type_: Some(Type::Count(Count {
            name_regex: ".*:rate$".into(),
            ..Default::default()
          })),
          name: "rate".into(),
          ..Default::default()
        },
        TrackingType {
          type_: Some(Type::TopK(TopK {
            group_by: Some(Group_by::NameRegex("".into())),
            top_k: 3,
            ..Default::default()
          })),
          name: "top metrics by name".into(),
          ..Default::default()
        },
        TrackingType {
          type_: Some(Type::TopK(TopK {
            group_by: Some(Group_by::TagNames(TagList {
              tags: vec!["project".into(), "facet".into()],
              ..Default::default()
            })),
            top_k: 5,
            ..Default::default()
          })),
          name: "top metrics by project".into(),
          ..Default::default()
        },
      ],
      ..Default::default()
    },
    context,
  )
  .unwrap();

  // Initial state should be empty.
  assert_admin(
    &helper.admin,
    r"
rate (approximate cardinality):
 populating

top metrics by name (approximate cardinality):
 populating

top metrics by project (approximate cardinality):
 populating
  ",
  )
  .await;

  // Add various metrics.
  make_mut(&mut helper.dispatcher)
    .expect_send()
    .once()
    .returning(|_| ());
  processor
    .clone()
    .recv_samples(vec![
      make_metric("foo:bar:baz", &[("a", "b"), ("project", "a")], 0),
      make_metric(
        "foo:bar:baz",
        &[("a", "c"), ("project", "a"), ("facet", "b")],
        0,
      ),
      make_metric(
        "foo:bar:baz",
        &[("a", "d"), ("project", "b"), ("facet", "c")],
        0,
      ),
      make_metric("foo:bar:rate", &[("a", "b")], 0),
      make_metric("foo:bar:rate", &[("a", "c")], 0),
      make_metric("blah:rate", &[("a", "b")], 0),
      make_metric("blah:rate", &[("a", "c")], 0),
    ])
    .await;

  // Since we haven't rotated, the output should still be empty.
  assert_admin(
    &helper.admin,
    r"
rate (approximate cardinality):
 populating

top metrics by name (approximate cardinality):
 populating

top metrics by project (approximate cardinality):
 populating
    ",
  )
  .await;

  // Rotating should get us cardinality for the basic count, but not for topk.
  tokio::time::sleep(Duration::from_secs(301)).await;

  assert_admin(
    &helper.admin,
    r"
rate (approximate cardinality):
 4.000000476837006

top metrics by name (approximate cardinality):
 populating

top metrics by project (approximate cardinality):
 populating
    ",
  )
  .await;

  // Add some new metrics, including once that would match the topk but won't be included.
  make_mut(&mut helper.dispatcher)
    .expect_send()
    .once()
    .returning(|_| ());
  processor
    .recv_samples(vec![
      make_metric("foo:bar:baz", &[("a", "b"), ("project", "a")], 0),
      make_metric(
        "foo:bar:baz",
        &[("a", "c"), ("project", "a"), ("facet", "b")],
        0,
      ),
      make_metric(
        "foo:bar:baz",
        &[("a", "d"), ("project", "b"), ("facet", "c")],
        0,
      ),
      make_metric("foo:bar:rate", &[("a", "b")], 0),
      make_metric("foo:bar:rate", &[("a", "c")], 0),
      make_metric("blah:rate", &[("a", "b")], 0),
      make_metric("blah:rate", &[("a", "c")], 0),
      make_metric("new:rate", &[("a", "b")], 0),
      make_metric("new:rate", &[("a", "c")], 0),
    ])
    .await;

  // Rotating should get us both types now.
  tokio::time::sleep(Duration::from_secs(301)).await;

  assert_admin(
    &helper.admin,
    r"
rate (approximate cardinality):
 6.000001072883094

top metrics by name (approximate cardinality):
 foo:bar:baz   3.0000002682208375
 blah:rate     2.0000001192092705
 foo:bar:rate  2.0000001192092705

top metrics by project (approximate cardinality):
 unknown_unknown  6.000001072883094
 a_b              1.00000002980232
 a_unknown        1.00000002980232
 b_c              1.00000002980232
    ",
  )
  .await;
}

#[tokio::test(start_paused = true)]
async fn emit_cardinality_gauges() {
  let (mut helper, context) = processor_factory_context_for_test();
  let processor = CardinalityTrackerProcessor::new(
    CardinalityTrackerConfig {
      tracking_types: vec![
        TrackingType {
          type_: Some(Type::Count(Count {
            name_regex: ".*:rate$".into(),
            ..Default::default()
          })),
          name: "rate".into(),
          ..Default::default()
        },
        TrackingType {
          type_: Some(Type::TopK(TopK {
            group_by: Some(Group_by::NameRegex("".into())),
            top_k: 1,
            ..Default::default()
          })),
          name: "top metric by name".into(),
          ..Default::default()
        },
      ],
      emit_cardinality_gauges: true,
      ..Default::default()
    },
    context,
  )
  .unwrap();

  assert_cardinality_gauge_absent(&helper, &[("tracker_name", "rate"), ("value", "")]);
  assert_cardinality_gauge_absent(
    &helper,
    &[
      ("tracker_name", "top metric by name"),
      ("value", "old:metric"),
    ],
  );

  make_mut(&mut helper.dispatcher)
    .expect_send()
    .once()
    .returning(|_| ());
  processor
    .clone()
    .recv_samples(vec![
      make_metric("old:metric", &[("a", "1")], 0),
      make_metric("old:metric", &[("a", "2")], 0),
      make_metric("old:metric", &[("a", "3")], 0),
      make_metric("old:rate", &[("a", "1")], 0),
      make_metric("old:rate", &[("a", "2")], 0),
    ])
    .await;

  assert_cardinality_gauge_absent(&helper, &[("tracker_name", "rate"), ("value", "")]);

  tokio::time::sleep(Duration::from_secs(301)).await;

  assert_cardinality_gauge(&helper, 2, &[("tracker_name", "rate"), ("value", "")]);
  assert_cardinality_gauge_absent(
    &helper,
    &[
      ("tracker_name", "top metric by name"),
      ("value", "old:metric"),
    ],
  );

  make_mut(&mut helper.dispatcher)
    .expect_send()
    .once()
    .returning(|_| ());
  processor
    .clone()
    .recv_samples(vec![
      make_metric("old:metric", &[("a", "1")], 0),
      make_metric("old:metric", &[("a", "2")], 0),
      make_metric("old:rate", &[("a", "3")], 0),
      make_metric("new:rate", &[("a", "1")], 0),
    ])
    .await;

  tokio::time::sleep(Duration::from_secs(301)).await;

  assert_cardinality_gauge(&helper, 2, &[("tracker_name", "rate"), ("value", "")]);
  assert_cardinality_gauge(
    &helper,
    2,
    &[
      ("tracker_name", "top metric by name"),
      ("value", "old:metric"),
    ],
  );

  make_mut(&mut helper.dispatcher)
    .expect_send()
    .once()
    .returning(|_| ());
  processor
    .clone()
    .recv_samples(vec![
      make_metric("new:metric", &[("a", "1")], 0),
      make_metric("new:metric", &[("a", "2")], 0),
      make_metric("new:metric", &[("a", "3")], 0),
      make_metric("new:metric", &[("a", "4")], 0),
      make_metric("later:rate", &[("a", "1")], 0),
    ])
    .await;

  tokio::time::sleep(Duration::from_secs(301)).await;

  assert_cardinality_gauge(
    &helper,
    2,
    &[
      ("tracker_name", "top metric by name"),
      ("value", "old:metric"),
    ],
  );

  make_mut(&mut helper.dispatcher)
    .expect_send()
    .once()
    .returning(|_| ());
  processor
    .recv_samples(vec![
      make_metric("new:metric", &[("a", "1")], 0),
      make_metric("new:metric", &[("a", "2")], 0),
      make_metric("new:metric", &[("a", "3")], 0),
      make_metric("final:rate", &[("a", "1")], 0),
    ])
    .await;

  tokio::time::sleep(Duration::from_secs(301)).await;

  assert_cardinality_gauge(&helper, 1, &[("tracker_name", "rate"), ("value", "")]);
  assert_cardinality_gauge(
    &helper,
    3,
    &[
      ("tracker_name", "top metric by name"),
      ("value", "new:metric"),
    ],
  );
  assert_cardinality_gauge_absent(
    &helper,
    &[
      ("tracker_name", "top metric by name"),
      ("value", "old:metric"),
    ],
  );
}
