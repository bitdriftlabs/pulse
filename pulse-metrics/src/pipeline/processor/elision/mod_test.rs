// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::*;
use crate::pipeline::metric_cache::MetricCache;
use crate::protos::metric::DownstreamId;
use crate::test::{
  make_carbon_wire_protocol,
  parse_carbon_metrics,
  processor_factory_context_for_test,
  ProcessorFactoryContextHelper,
};
use bd_test_helpers::make_mut;
use matches::assert_matches;
use mockall::predicate;
use std::time::{Duration, Instant};

fn make_emit_ratio(ratio: f64) -> EmitConfig {
  EmitConfig {
    emit_type: Emit_type::Ratio(ratio).into(),
    ..Default::default()
  }
}

fn make_emit_interval(seconds: u64) -> EmitConfig {
  EmitConfig {
    emit_type: Emit_type::Interval(Duration::from_secs(seconds).into()).into(),
    ..Default::default()
  }
}

async fn setup_elision_processor(
  regex_overrides: Option<Vec<RegexOverrideConfig>>,
  emit: Option<EmitConfig>,
) -> (Arc<ElisionProcessor>, ProcessorFactoryContextHelper) {
  let config = ElisionConfig {
    emit: Some(emit.unwrap_or_else(|| make_emit_ratio(0.5))).into(),
    regex_overrides: regex_overrides.unwrap_or_default(),
    analyze_mode: false,
    zero: Some(ZeroElisionConfig::default()).into(),
    blocklist: None.into(),
    ..Default::default()
  };
  let (helper, factory) = processor_factory_context_for_test();

  (
    ElisionProcessor::new(config, factory).await.unwrap(),
    helper,
  )
}

fn processor_decide(
  processor: &ElisionProcessor,
  carbon: bytes::Bytes,
  metric_cache: &Arc<MetricCache>,
) -> SkipDecision {
  let mut sample = ParsedMetric::try_from_wire_protocol(
    carbon,
    &make_carbon_wire_protocol(),
    Instant::now(),
    DownstreamId::LocalOrigin,
  )
  .unwrap();
  sample.initialize_cache(metric_cache);
  processor.decide(&sample)
}

#[tokio::test]
async fn prefixes_include() {
  let (processor, helper) = setup_elision_processor(None, None).await;
  let result = processor_decide(
    &processor,
    "node_something.count 0 0 source=hello".into(),
    &helper.metric_cache,
  );
  assert_eq!(result, SkipDecision::Output);
}

#[tokio::test]
async fn prefixes_exclude() {
  let regex_overrides: Vec<RegexOverrideConfig> = vec![RegexOverrideConfig {
    regex: "^node_request.*".into(),
    emit: Some(make_emit_ratio(1.0)).into(),
    ..Default::default()
  }];
  let (processor, helper) = setup_elision_processor(Some(regex_overrides), None).await;
  let result = processor_decide(
    &processor,
    "node_request.count 0 0 source=hello".into(),
    &helper.metric_cache,
  );
  assert_eq!(result, SkipDecision::Output);
}

#[tokio::test]
async fn regex_overrides() {
  let regex_overrides: Vec<RegexOverrideConfig> = vec![RegexOverrideConfig {
    regex: "^node_request.*".into(),
    emit: Some(make_emit_ratio(0.2)).into(),
    ..Default::default()
  }];
  let (processor, helper) = setup_elision_processor(Some(regex_overrides), None).await;
  let result = processor_decide(
    &processor,
    "node_request.count 0 0 source=hello".into(),
    &helper.metric_cache,
  );
  assert_eq!(result, SkipDecision::Output);
}

#[tokio::test]
async fn emit_interval() {
  let (processor, helper) = setup_elision_processor(None, Some(make_emit_interval(60))).await;

  let input = bytes::Bytes::from_static(b"node_request.count 0 100 source=hello");
  let result = processor_decide(&processor, input, &helper.metric_cache);
  assert_eq!(result, SkipDecision::Output);

  let input = bytes::Bytes::from_static(b"node_request.count 0 130 source=hello");
  let result = processor_decide(&processor, input, &helper.metric_cache);
  assert_matches!(result, SkipDecision::DoNotOutput);

  let input = bytes::Bytes::from_static(b"node_request.count 0 159 source=hello");
  let result = processor_decide(&processor, input, &helper.metric_cache);
  assert_matches!(result, SkipDecision::DoNotOutput);

  let input = bytes::Bytes::from_static(b"node_request.count 0 160 source=hello");
  let result = processor_decide(&processor, input, &helper.metric_cache);
  assert_matches!(result, SkipDecision::Output);

  let input = bytes::Bytes::from_static(b"node_request.count 0 161 source=hello");
  let result = processor_decide(&processor, input, &helper.metric_cache);
  assert_matches!(result, SkipDecision::DoNotOutput);

  let input = bytes::Bytes::from_static(b"node_request.count 0 219 source=hello");
  let result = processor_decide(&processor, input, &helper.metric_cache);
  assert_matches!(result, SkipDecision::DoNotOutput);

  let input = bytes::Bytes::from_static(b"node_request.count 0 225 source=hello");
  let result = processor_decide(&processor, input, &helper.metric_cache);
  assert_matches!(result, SkipDecision::Output);
}

#[tokio::test]
async fn processor_zeroes() {
  let (processor, mut helper) = setup_elision_processor(None, None).await;

  let input: Vec<ParsedMetric> = parse_carbon_metrics(&[
    "node_request.count 0 100 source=hello",
    "node_request.count 0 101 source=hello",
    "node_request.count 0 102 source=hello",
    "node_request.count 0 103 source=hello",
    "node_request.count 0 104 source=hello",
  ]);

  let output = vec![input[0].clone(), input[1].clone(), input[3].clone()];

  make_mut(&mut helper.dispatcher)
    .expect_send()
    .with(predicate::eq(output))
    .returning(|_| ());

  processor.recv_samples(input).await;
}

#[tokio::test]
async fn processor_mixed_values() {
  let (processor, mut helper) = setup_elision_processor(None, None).await;

  let input: Vec<ParsedMetric> = parse_carbon_metrics(&[
    "node_request.count 0 100 source=hello",
    "node_request.count 1 100 source=hello",
    "node_request.count 1 101 source=hello",
    "node_request.count 0 102 source=hello",
    "node_request.count 0 103 source=hello",
    "node_request.count 0 104 source=hello",
  ]);

  let output = vec![
    input[0].clone(),
    input[1].clone(),
    input[2].clone(),
    input[3].clone(),
    input[5].clone(),
  ];

  make_mut(&mut helper.dispatcher)
    .expect_send()
    .with(predicate::eq(output))
    .returning(|_| ());

  processor.recv_samples(input).await;
}

#[tokio::test]
async fn last_elided() {
  let (processor, helper) = setup_elision_processor(None, Some(make_emit_ratio(0.25))).await;

  let name = "node_request.count";
  let input = bytes::Bytes::from_static(b"node_request.count 0 100 source=hello");
  let result = processor_decide(&processor, input, &helper.metric_cache);
  assert_eq!(result, SkipDecision::Output);
  assert_eq!(processor.get_last_elided(name).unwrap(), Some(0));

  let input = bytes::Bytes::from_static(b"node_request.count 0 101 source=hello");
  let result = processor_decide(&processor, input, &helper.metric_cache);
  assert_matches!(result, SkipDecision::DoNotOutput);
  assert_eq!(processor.get_last_elided(name).unwrap(), Some(101));

  let input = bytes::Bytes::from_static(b"node_request.count 0 102 source=hello");
  let result = processor_decide(&processor, input, &helper.metric_cache);
  assert_matches!(result, SkipDecision::DoNotOutput);
  assert_eq!(processor.get_last_elided(name).unwrap(), Some(102));

  // Last elided shouldn't be updated when the line isn't elided.
  let input = bytes::Bytes::from_static(b"node_request.count 0 103 source=hello");
  let result = processor_decide(&processor, input, &helper.metric_cache);
  assert_matches!(result, SkipDecision::Output);
  assert_eq!(processor.get_last_elided(name).unwrap(), Some(102));
}

#[test]
fn get_override() {
  let regex_overrides: Vec<RegexOverrideConfig> = vec![
    RegexOverrideConfig {
      regex: "^node_.*".into(),
      emit: Some(make_emit_ratio(0.2)).into(),
      ..Default::default()
    },
    RegexOverrideConfig {
      regex: "^prod.*".into(),
      emit: Some(make_emit_ratio(0.9)).into(),
      ..Default::default()
    },
  ];
  let override_config = OverrideConfig::new(&regex_overrides).unwrap();

  assert!(override_config
    .get_override(b"staging.app.something")
    .is_none());
  assert_eq!(
    &make_emit_ratio(0.2),
    override_config.get_override(b"node_tests.sum").unwrap()
  );
  assert_eq!(
    &make_emit_ratio(0.9),
    override_config.get_override(b"production.app.sum").unwrap()
  );
}
