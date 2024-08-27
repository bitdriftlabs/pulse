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
use fst::SetBuilder;

fn mock_metric(name: bytes::Bytes) -> Metric {
  Metric::new(
    MetricId::new(name, None, vec![], false).unwrap(),
    None,
    1_660_557_239,
    MetricValue::Simple(0.),
  )
}

async fn make_fst_filter(
  metrics: &[&[u8]],
  match_decision: MetricFilterDecision,
  prom: bool,
  name: String,
) -> PollFilter {
  let mut builder = SetBuilder::memory();
  for metric in metrics {
    builder.insert(metric).unwrap();
  }

  PollFilter::new_for_test(
    FstFilterProvider::new(builder.into_inner().unwrap().into())
      .await
      .unwrap(),
    match_decision,
    prom,
    name,
  )
}

#[tokio::test]
async fn metric_filter_decide_fail() {
  let blocked_metrics: Vec<&[u8]> = vec![b"alpha", b"alpha.beta"];
  let metric_filter = make_fst_filter(
    &blocked_metrics,
    MetricFilterDecision::Fail,
    false,
    String::default(),
  )
  .await;
  assert_eq!(
    metric_filter.decide(&mock_metric(Bytes::from_static(b"foo.bar")),),
    MetricFilterDecision::NotCovered
  );
  assert_eq!(
    metric_filter.decide(&mock_metric(Bytes::from_static(b"alpha")),),
    MetricFilterDecision::Fail
  );
  assert_eq!(
    metric_filter.decide(&mock_metric(Bytes::from_static(b"alpha.beta")),),
    MetricFilterDecision::Fail
  );
  assert_eq!(
    metric_filter.decide(&mock_metric(Bytes::from_static(b"beta.gamma")),),
    MetricFilterDecision::NotCovered
  );
}

#[tokio::test]
async fn metric_filter_decide_pass() {
  let blocked_metrics: Vec<&[u8]> = vec![b"alpha", b"alpha.beta"];
  let metric_filter = make_fst_filter(
    &blocked_metrics,
    MetricFilterDecision::Pass,
    false,
    String::default(),
  )
  .await;
  assert_eq!(
    metric_filter.decide(&mock_metric(Bytes::from_static(b"foo.bar")),),
    MetricFilterDecision::NotCovered
  );
  assert_eq!(
    metric_filter.decide(&mock_metric(Bytes::from_static(b"alpha")),),
    MetricFilterDecision::Pass
  );
  assert_eq!(
    metric_filter.decide(&mock_metric(Bytes::from_static(b"alphaa")),),
    MetricFilterDecision::NotCovered
  );
  assert_eq!(
    metric_filter.decide(&mock_metric(Bytes::from_static(b"alpha.beta")),),
    MetricFilterDecision::Pass
  );
  assert_eq!(
    metric_filter.decide(&mock_metric(Bytes::from_static(b"beta.gamma")),),
    MetricFilterDecision::NotCovered
  );
}

#[tokio::test]
async fn metric_filter_decide_block_prom() {
  let blocked_metrics: Vec<&[u8]> = vec![
    b"alpha",
    b"alpha:beta",
    b"alpha:beta:gamma",
    b"beta:gamma",
    b"beta:gamma:delta",
    b"beta_gamma_delta039:sum",
  ];
  let metric_filter = make_fst_filter(
    &blocked_metrics,
    MetricFilterDecision::Fail,
    true,
    String::default(),
  )
  .await;

  assert_eq!(
    metric_filter.decide(&mock_metric(Bytes::from_static(b"foo.bar")),),
    MetricFilterDecision::NotCovered
  );
  assert_eq!(
    metric_filter.decide(&mock_metric(Bytes::from_static(b"alpha")),),
    MetricFilterDecision::Fail
  );
  assert_eq!(
    metric_filter.decide(&mock_metric(Bytes::from_static(b"alpha.beta")),),
    MetricFilterDecision::Fail
  );
  assert_eq!(
    metric_filter.decide(&mock_metric(Bytes::from_static(b"beta.gamma")),),
    MetricFilterDecision::Fail
  );
  assert_eq!(
    metric_filter.decide(&mock_metric(Bytes::from_static(b"beta/gamma*delta039.sum")),),
    MetricFilterDecision::Fail
  );
}
