// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::pipeline::processor::PipelineProcessor;
use crate::pipeline::processor::drop::DropProcessor;
use crate::test::{make_metric, processor_factory_context_for_test};
use bd_test_helpers::make_mut;
use drop::drop_processor_config::Config_source;
use drop::drop_rule::drop_condition::Condition_type;
use drop::drop_rule::string_match::String_match_type;
use drop::drop_rule::value_match::Value_match_type;
use drop::drop_rule::{
  AndMatch,
  DropCondition,
  DropMode,
  SimpleValueMatch,
  StringMatch,
  TagMatch,
  TimestampAgeMatch,
  ValueMatch,
  ValueMatchOperator,
};
use drop::{DropConfig, DropProcessorConfig, DropRule};
use prometheus::labels;
use pulse_protobuf::protos::pulse::config::processor::v1::drop;
use std::sync::Arc;

fn make_exact_match(name: &str) -> DropCondition {
  DropCondition {
    condition_type: Some(Condition_type::MetricName(StringMatch {
      string_match_type: Some(String_match_type::Exact(name.to_string().into())),
      ..Default::default()
    })),
    ..Default::default()
  }
}

fn make_regex_match(name: &str) -> DropCondition {
  DropCondition {
    condition_type: Some(Condition_type::MetricName(StringMatch {
      string_match_type: Some(String_match_type::Regex(name.to_string().into())),
      ..Default::default()
    })),
    ..Default::default()
  }
}

fn make_not_match(condition: DropCondition) -> DropCondition {
  DropCondition {
    condition_type: Some(Condition_type::NotMatch(Box::new(condition))),
    ..Default::default()
  }
}

fn make_tag_match(name: &str) -> DropCondition {
  DropCondition {
    condition_type: Some(Condition_type::TagMatch(TagMatch {
      tag_name: name.to_string().into(),
      ..Default::default()
    })),
    ..Default::default()
  }
}

fn make_tag_value_exact_match(tag_name: &str, tag_value: &str) -> DropCondition {
  DropCondition {
    condition_type: Some(Condition_type::TagMatch(TagMatch {
      tag_name: tag_name.to_string().into(),
      tag_value: Some(StringMatch {
        string_match_type: Some(String_match_type::Exact(tag_value.to_string().into())),
        ..Default::default()
      })
      .into(),
      ..Default::default()
    })),
    ..Default::default()
  }
}

fn make_tag_value_regex_match(tag_name: &str, tag_value: &str) -> DropCondition {
  DropCondition {
    condition_type: Some(Condition_type::TagMatch(TagMatch {
      tag_name: tag_name.to_string().into(),
      tag_value: Some(StringMatch {
        string_match_type: Some(String_match_type::Regex(tag_value.to_string().into())),
        ..Default::default()
      })
      .into(),
      ..Default::default()
    })),
    ..Default::default()
  }
}

fn make_and_match(conditions: Vec<DropCondition>) -> DropCondition {
  DropCondition {
    condition_type: Some(Condition_type::AndMatch(AndMatch {
      conditions,
      ..Default::default()
    })),
    ..Default::default()
  }
}

fn make_timestamp_age_match(max_age_seconds: u64) -> DropCondition {
  DropCondition {
    condition_type: Some(Condition_type::TimestampAgeMatch(TimestampAgeMatch {
      max_age_seconds,
      ..Default::default()
    })),
    ..Default::default()
  }
}

#[tokio::test]
async fn regex_vs_exact() {
  let (mut helper, context) = processor_factory_context_for_test();
  let processor = Arc::new(
    DropProcessor::new(
      DropProcessorConfig {
        config_source: Some(Config_source::Inline(DropConfig {
          rules: vec![DropRule {
            name: "rule1".into(),
            mode: DropMode::ENABLED.into(),
            conditions: vec![make_exact_match("exact_name")],
            ..Default::default()
          }],
          ..Default::default()
        })),
        ..Default::default()
      },
      context,
    )
    .await
    .unwrap(),
  );

  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .returning(|metrics| {
      assert_eq!(metrics, vec![make_metric("exact_name_with_more", &[], 0),]);
    });
  processor
    .clone()
    .recv_samples(vec![
      make_metric("exact_name", &[], 0),
      make_metric("exact_name_with_more", &[], 0),
    ])
    .await;
  helper.stats_helper.assert_counter_eq(
    1,
    "processor:dropped",
    &labels! { "rule_name" => "rule1", "mode" => "enabled" },
  );
}

#[tokio::test]
async fn not() {
  let (mut helper, context) = processor_factory_context_for_test();
  let processor = Arc::new(
    DropProcessor::new(
      DropProcessorConfig {
        config_source: Some(Config_source::Inline(DropConfig {
          rules: vec![DropRule {
            name: "rule1".into(),
            mode: DropMode::ENABLED.into(),
            conditions: vec![make_not_match(make_exact_match("exact_name"))],
            ..Default::default()
          }],
          ..Default::default()
        })),
        ..Default::default()
      },
      context,
    )
    .await
    .unwrap(),
  );

  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .returning(|metrics| {
      assert_eq!(metrics, vec![make_metric("exact_name", &[], 0),]);
    });
  processor
    .clone()
    .recv_samples(vec![
      make_metric("dropped", &[], 0),
      make_metric("exact_name", &[], 0),
    ])
    .await;
  helper.stats_helper.assert_counter_eq(
    1,
    "processor:dropped",
    &labels! { "rule_name" => "rule1", "mode" => "enabled" },
  );
}

#[tokio::test]
async fn all() {
  let (mut helper, context) = processor_factory_context_for_test();
  let processor = Arc::new(
    DropProcessor::new(
      DropProcessorConfig {
        config_source: Some(Config_source::Inline(DropConfig {
          rules: vec![
            DropRule {
              name: "rule1".into(),
              mode: DropMode::ENABLED.into(),
              conditions: vec![
                make_exact_match("exact_name"),
                make_regex_match("regex_name.*"),
                make_tag_match("tag_present"),
                make_tag_value_exact_match("tag_exact", "exact_value"),
                make_tag_value_regex_match("tag_regex", "regex_value.*"),
              ],
              ..Default::default()
            },
            DropRule {
              name: "rule2".into(),
              mode: DropMode::TESTING.into(),
              conditions: vec![make_and_match(vec![
                make_exact_match("value_match_name"),
                DropCondition {
                  condition_type: Some(Condition_type::ValueMatch(ValueMatch {
                    value_match_type: Some(Value_match_type::SimpleValue(SimpleValueMatch {
                      target: 100.0,
                      operator: ValueMatchOperator::NOT_EQUAL.into(),
                      ..Default::default()
                    })),
                    ..Default::default()
                  })),
                  ..Default::default()
                },
              ])],
              ..Default::default()
            },
          ],
          ..Default::default()
        })),
        ..Default::default()
      },
      context,
    )
    .await
    .unwrap(),
  );

  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .returning(|metrics| {
      assert_eq!(
        metrics,
        vec![
          make_metric("not_dropped", &[], 0),
          make_metric("tag_exact", &[("tag_exact", "not_exact")], 0),
          make_metric("value_match_name", &[], 0),
          make_metric("value_match_name", &[], 100),
        ]
      );
    });
  processor
    .clone()
    .recv_samples(vec![
      make_metric("not_dropped", &[], 0),
      make_metric("exact_name", &[], 0),
      make_metric("regex_name123", &[], 0),
      make_metric("tag_present", &[("tag_present", "something")], 0),
      make_metric("tag_exact", &[("tag_exact", "exact_value")], 0),
      make_metric("tag_exact", &[("tag_exact", "not_exact")], 0),
      make_metric("tag_regex", &[("tag_regex", "regex_value123")], 0),
      make_metric("value_match_name", &[], 0),
      make_metric("value_match_name", &[], 100),
    ])
    .await;
  helper.stats_helper.assert_counter_eq(
    5,
    "processor:dropped",
    &labels! { "rule_name" => "rule1", "mode" => "enabled" },
  );
  helper.stats_helper.assert_counter_eq(
    2,
    "processor:dropped",
    &labels! { "rule_name" => "rule2", "mode" => "testing" },
  );
}

#[tokio::test]
async fn timestamp_age_match() {
  let (mut helper, context) = processor_factory_context_for_test();
  let processor = Arc::new(
    DropProcessor::new(
      DropProcessorConfig {
        config_source: Some(Config_source::Inline(DropConfig {
          rules: vec![DropRule {
            name: "rule1".into(),
            mode: DropMode::ENABLED.into(),
            conditions: vec![make_timestamp_age_match(3600)],
            ..Default::default()
          }],
          ..Default::default()
        })),
        ..Default::default()
      },
      context,
    )
    .await
    .unwrap(),
  );

  let current_time = crate::protos::metric::default_timestamp();
  let old_timestamp = current_time - 7200; // 2 hours old
  let recent_timestamp = current_time - 1800; // 30 minutes old
  let future_timestamp = current_time + 3600; // 1 hour in the future

  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .returning(|metrics| {
      assert_eq!(metrics.len(), 2);
    });
  processor
    .clone()
    .recv_samples(vec![
      make_metric("old_metric", &[], old_timestamp),
      make_metric("recent_metric", &[], recent_timestamp),
      make_metric("future_metric", &[], future_timestamp),
    ])
    .await;
  helper.stats_helper.assert_counter_eq(
    1,
    "processor:dropped",
    &labels! { "rule_name" => "rule1", "mode" => "enabled" },
  );
}

#[tokio::test]
async fn timestamp_age_match_with_and() {
  let (mut helper, context) = processor_factory_context_for_test();
  let processor = Arc::new(
    DropProcessor::new(
      DropProcessorConfig {
        config_source: Some(Config_source::Inline(DropConfig {
          rules: vec![DropRule {
            name: "rule1".into(),
            mode: DropMode::ENABLED.into(),
            conditions: vec![make_and_match(vec![
              make_exact_match("test_metric"),
              make_timestamp_age_match(3600),
            ])],
            ..Default::default()
          }],
          ..Default::default()
        })),
        ..Default::default()
      },
      context,
    )
    .await
    .unwrap(),
  );

  let current_time = crate::protos::metric::default_timestamp();
  let old_timestamp = current_time - 7200; // 2 hours old
  let recent_timestamp = current_time - 1800; // 30 minutes old

  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .returning(|metrics| {
      assert_eq!(metrics.len(), 3);
    });
  processor
    .clone()
    .recv_samples(vec![
      make_metric("test_metric", &[], old_timestamp),
      make_metric("test_metric", &[], recent_timestamp),
      make_metric("other_metric", &[], old_timestamp),
      make_metric("other_metric", &[], recent_timestamp),
    ])
    .await;
  helper.stats_helper.assert_counter_eq(
    1,
    "processor:dropped",
    &labels! { "rule_name" => "rule1", "mode" => "enabled" },
  );
}

#[tokio::test]
async fn timestamp_age_match_with_not() {
  let (mut helper, context) = processor_factory_context_for_test();
  let processor = Arc::new(
    DropProcessor::new(
      DropProcessorConfig {
        config_source: Some(Config_source::Inline(DropConfig {
          rules: vec![DropRule {
            name: "rule1".into(),
            mode: DropMode::ENABLED.into(),
            conditions: vec![make_not_match(make_timestamp_age_match(3600))],
            ..Default::default()
          }],
          ..Default::default()
        })),
        ..Default::default()
      },
      context,
    )
    .await
    .unwrap(),
  );

  let current_time = crate::protos::metric::default_timestamp();
  let old_timestamp = current_time - 7200; // 2 hours old
  let recent_timestamp = current_time - 1800; // 30 minutes old

  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .returning(|metrics| {
      assert_eq!(metrics.len(), 1);
    });
  processor
    .clone()
    .recv_samples(vec![
      make_metric("old_metric", &[], old_timestamp),
      make_metric("recent_metric", &[], recent_timestamp),
    ])
    .await;
  helper.stats_helper.assert_counter_eq(
    1,
    "processor:dropped",
    &labels! { "rule_name" => "rule1", "mode" => "enabled" },
  );
}

#[tokio::test]
async fn timestamp_age_match_testing_mode() {
  let (mut helper, context) = processor_factory_context_for_test();
  let processor = Arc::new(
    DropProcessor::new(
      DropProcessorConfig {
        config_source: Some(Config_source::Inline(DropConfig {
          rules: vec![DropRule {
            name: "rule1".into(),
            mode: DropMode::TESTING.into(),
            conditions: vec![make_timestamp_age_match(3600)],
            ..Default::default()
          }],
          ..Default::default()
        })),
        ..Default::default()
      },
      context,
    )
    .await
    .unwrap(),
  );

  let current_time = crate::protos::metric::default_timestamp();
  let old_timestamp = current_time - 7200; // 2 hours old

  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .returning(|metrics| {
      // In testing mode, nothing should be dropped
      assert_eq!(metrics.len(), 1);
    });
  processor
    .clone()
    .recv_samples(vec![make_metric("old_metric", &[], old_timestamp)])
    .await;
  helper.stats_helper.assert_counter_eq(
    1,
    "processor:dropped",
    &labels! { "rule_name" => "rule1", "mode" => "testing" },
  );
}

#[tokio::test]
async fn warn_interval() {
  let (mut helper, context) = processor_factory_context_for_test();
  let processor = Arc::new(
    DropProcessor::new(
      DropProcessorConfig {
        config_source: Some(Config_source::Inline(DropConfig {
          rules: vec![DropRule {
            name: "rule1".into(),
            mode: DropMode::ENABLED.into(),
            conditions: vec![make_exact_match("drop_this")],
            warn_interval_seconds: 60,
            ..Default::default()
          }],
          ..Default::default()
        })),
        ..Default::default()
      },
      context,
    )
    .await
    .unwrap(),
  );

  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .returning(|metrics| {
      assert_eq!(metrics.len(), 1);
    });
  processor
    .clone()
    .recv_samples(vec![
      make_metric("drop_this", &[], 0),
      make_metric("keep_this", &[], 0),
    ])
    .await;
  helper.stats_helper.assert_counter_eq(
    1,
    "processor:dropped",
    &labels! { "rule_name" => "rule1", "mode" => "enabled" },
  );
}
