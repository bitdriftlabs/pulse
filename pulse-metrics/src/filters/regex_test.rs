// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::*;
use crate::filters::filter::MetricFilterDecision;
use crate::filters::poll_filter::PollFilter;
use crate::protos::metric::{Metric, MetricId, MetricValue};
use bytes::Bytes;

fn new_metric(name: bytes::Bytes) -> Metric {
  Metric::new(
    MetricId::new(name, None, vec![], false).unwrap(),
    None,
    1_660_557_239,
    MetricValue::Simple(0.),
  )
}

async fn make_regex_filter(
  metrics: Vec<&str>,
  match_decision: MetricFilterDecision,
  prom: bool,
  name: String,
) -> PollFilter {
  PollFilter::new_for_test(
    RegexFilterProvider::new(metrics.join("\n").into())
      .await
      .unwrap(),
    match_decision,
    prom,
    name,
  )
}

#[tokio::test]
async fn metric_filter_new() {
  let match_patterns = vec!["^alpha.beta..*$", "^.*.gamma.delta$"];
  make_regex_filter(
    match_patterns,
    MetricFilterDecision::Pass,
    false,
    String::from("regex_filter"),
  )
  .await;
}

#[tokio::test]
async fn metric_filter_decide_pass() {
  let match_patterns = vec!["^alpha.beta..*$", "^.*.gamma.delta$"];
  let metric_filter = make_regex_filter(
    match_patterns,
    MetricFilterDecision::Pass,
    false,
    String::from("regex_filter"),
  )
  .await;

  assert_eq!(
    metric_filter.decide(&new_metric(Bytes::from_static(b"foo.bar")),),
    MetricFilterDecision::NotCovered
  );
  assert_eq!(
    metric_filter.decide(&new_metric(Bytes::from_static(
      b"alpha.beta.something.else"
    )),),
    MetricFilterDecision::Pass
  );
  assert_eq!(
    metric_filter.decide(&new_metric(Bytes::from_static(b"foo.bar.gamma.delta")),),
    MetricFilterDecision::Pass
  );
  assert_eq!(
    metric_filter.decide(&new_metric(Bytes::from_static(b"alpha.beta")),),
    MetricFilterDecision::NotCovered
  );
}

#[tokio::test]
async fn metric_filter_decide_fail() {
  let match_patterns = vec!["^alpha.beta..*$", "^.*.gamma.delta$"];
  let metric_filter = make_regex_filter(
    match_patterns,
    MetricFilterDecision::Fail,
    false,
    String::from("regex_filter"),
  )
  .await;

  assert_eq!(
    metric_filter.decide(&new_metric(Bytes::from_static(b"foo.bar")),),
    MetricFilterDecision::NotCovered
  );
  assert_eq!(
    metric_filter.decide(&new_metric(Bytes::from_static(
      b"alpha.beta.something.else"
    )),),
    MetricFilterDecision::Fail
  );
  assert_eq!(
    metric_filter.decide(&new_metric(Bytes::from_static(b"foo.bar.gamma.delta")),),
    MetricFilterDecision::Fail
  );
  assert_eq!(
    metric_filter.decide(&new_metric(Bytes::from_static(b"alpha.beta")),),
    MetricFilterDecision::NotCovered
  );
}

#[tokio::test]
async fn metric_filter_decide_pass_prom() {
  let match_patterns = vec![
    "^alpha:beta:.*$",
    "^.*:gamma:delta$",
    "^.*_gamma_delta[^:]*:sum$",
  ];
  let metric_filter = make_regex_filter(
    match_patterns,
    MetricFilterDecision::Pass,
    true,
    String::from("regex_filter"),
  )
  .await;

  assert_eq!(
    metric_filter.decide(&new_metric(Bytes::from_static(b"foo.bar")),),
    MetricFilterDecision::NotCovered
  );
  assert_eq!(
    metric_filter.decide(&new_metric(Bytes::from_static(b"alpha.beta")),),
    MetricFilterDecision::NotCovered
  );
  assert_eq!(
    metric_filter.decide(&new_metric(Bytes::from_static(b"alpha.beta.something")),),
    MetricFilterDecision::Pass
  );
  assert_eq!(
    metric_filter.decide(&new_metric(Bytes::from_static(b"beta.gamma.delta")),),
    MetricFilterDecision::Pass
  );
  assert_eq!(
    metric_filter.decide(&new_metric(Bytes::from_static(b"beta/gamma*delta039.sum")),),
    MetricFilterDecision::Pass
  );
}
