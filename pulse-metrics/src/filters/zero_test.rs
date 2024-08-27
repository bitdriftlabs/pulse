// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::filters::filter::{MetricFilter, MetricFilterDecision};
use crate::filters::zero::ZeroFilter;
use crate::protos::metric::{
  CounterType,
  HistogramData,
  Metric,
  MetricId,
  MetricType,
  MetricValue,
  SummaryData,
};
use bytes::Bytes;
use elision_config::zero_elision_config::counters::AbsoluteCounters;
use elision_config::zero_elision_config::{Counters, Histograms};
use elision_config::ZeroElisionConfig;
use pulse_protobuf::protos::pulse::config::processor::v1::elision::elision_config;

fn mock_metric(metric_value: MetricValue, mtype: Option<MetricType>) -> Metric {
  Metric::new(
    MetricId::new(Bytes::new(), mtype, vec![], false).unwrap(),
    None,
    1_660_557_239,
    metric_value,
  )
}

#[test]
fn decide_absolute_counter_zero_elide_if_unchanged() {
  let filter = ZeroFilter::new(
    "",
    &ZeroElisionConfig {
      counters: Some(Counters {
        absolute_counters: Some(AbsoluteCounters {
          elide_if_no_change: true,
          ..Default::default()
        })
        .into(),
        ..Default::default()
      })
      .into(),
      ..Default::default()
    },
  );
  assert_eq!(
    filter.decide(
      &mock_metric(
        MetricValue::Simple(100_f64),
        Some(MetricType::Counter(CounterType::Absolute))
      ),
      &Some(MetricValue::Simple(100_f64)),
      Some(MetricType::Counter(CounterType::Absolute)),
    ),
    MetricFilterDecision::Fail
  );

  assert_eq!(
    filter.decide(
      &mock_metric(
        MetricValue::Simple(100_f64),
        Some(MetricType::Counter(CounterType::Absolute))
      ),
      &Some(MetricValue::Simple(101_f64)),
      Some(MetricType::Counter(CounterType::Absolute)),
    ),
    MetricFilterDecision::NotCovered
  );

  assert_eq!(
    filter.decide(
      &mock_metric(
        MetricValue::Simple(100_f64),
        Some(MetricType::Counter(CounterType::Delta))
      ),
      &Some(MetricValue::Simple(100_f64)),
      Some(MetricType::Counter(CounterType::Delta)),
    ),
    MetricFilterDecision::NotCovered
  );
}

#[test]
fn decide_histogram_zero() {
  let filter = ZeroFilter::new("", &ZeroElisionConfig::default());
  assert_eq!(
    filter.decide(
      &mock_metric(
        MetricValue::Histogram(HistogramData {
          buckets: vec![],
          sample_count: 0.0,
          sample_sum: 0.0,
        }),
        Some(MetricType::Histogram)
      ),
      &Some(MetricValue::Histogram(HistogramData {
        buckets: vec![],
        sample_count: 0.0,
        sample_sum: 0.0,
      })),
      Some(MetricType::Histogram),
    ),
    MetricFilterDecision::Fail
  );

  assert_eq!(
    filter.decide(
      &mock_metric(
        MetricValue::Histogram(HistogramData {
          buckets: vec![],
          sample_count: 1.0,
          sample_sum: 0.0,
        }),
        Some(MetricType::Histogram)
      ),
      &Some(MetricValue::Histogram(HistogramData {
        buckets: vec![],
        sample_count: 1.0,
        sample_sum: 0.0,
      })),
      Some(MetricType::Histogram),
    ),
    MetricFilterDecision::NotCovered
  );
}

#[test]
fn decide_histogram_zero_elide_if_unchanged() {
  let filter = ZeroFilter::new(
    "",
    &ZeroElisionConfig {
      histograms: Some(Histograms {
        elide_if_no_change: true,
        ..Default::default()
      })
      .into(),
      ..Default::default()
    },
  );
  assert_eq!(
    filter.decide(
      &mock_metric(
        MetricValue::Histogram(HistogramData {
          buckets: vec![],
          sample_count: 1.0,
          sample_sum: 0.0,
        }),
        Some(MetricType::Histogram)
      ),
      &Some(MetricValue::Histogram(HistogramData {
        buckets: vec![],
        sample_count: 1.0,
        sample_sum: 0.0,
      })),
      Some(MetricType::Histogram),
    ),
    MetricFilterDecision::Fail
  );

  assert_eq!(
    filter.decide(
      &mock_metric(
        MetricValue::Histogram(HistogramData {
          buckets: vec![],
          sample_count: 1.0,
          sample_sum: 0.0,
        }),
        Some(MetricType::Histogram)
      ),
      &Some(MetricValue::Histogram(HistogramData {
        buckets: vec![],
        sample_count: 2.0,
        sample_sum: 0.0,
      })),
      Some(MetricType::Histogram),
    ),
    MetricFilterDecision::NotCovered
  );

  assert_eq!(
    filter.decide(
      &mock_metric(
        MetricValue::Histogram(HistogramData {
          buckets: vec![],
          sample_count: 0.0,
          sample_sum: 0.0,
        }),
        Some(MetricType::Histogram)
      ),
      &Some(MetricValue::Histogram(HistogramData {
        buckets: vec![],
        sample_count: 0.0,
        sample_sum: 0.0,
      })),
      Some(MetricType::Histogram),
    ),
    MetricFilterDecision::Fail
  );
}

#[test]
fn decide_summary_zero() {
  let filter = ZeroFilter::new("", &ZeroElisionConfig::default());
  assert_eq!(
    filter.decide(
      &mock_metric(
        MetricValue::Summary(SummaryData {
          quantiles: vec![],
          sample_count: 1.0,
          sample_sum: 0.0,
        }),
        Some(MetricType::Summary)
      ),
      &Some(MetricValue::Summary(SummaryData {
        quantiles: vec![],
        sample_count: 1.0,
        sample_sum: 0.0,
      })),
      Some(MetricType::Summary),
    ),
    MetricFilterDecision::Fail
  );

  assert_eq!(
    filter.decide(
      &mock_metric(
        MetricValue::Summary(SummaryData {
          quantiles: vec![],
          sample_count: 1.0,
          sample_sum: 0.0,
        }),
        Some(MetricType::Summary)
      ),
      &Some(MetricValue::Summary(SummaryData {
        quantiles: vec![],
        sample_count: 2.0,
        sample_sum: 0.0,
      })),
      Some(MetricType::Summary),
    ),
    MetricFilterDecision::NotCovered
  );
}

#[test]
fn decide_zero_zero() {
  let filter = ZeroFilter::new("", &ZeroElisionConfig::default());
  assert_eq!(
    filter.decide(
      &mock_metric(MetricValue::Simple(0_f64), None),
      &Some(MetricValue::Simple(0_f64)),
      None
    ),
    MetricFilterDecision::Fail
  );
}

#[test]
fn decide_value_to_zero() {
  let filter = ZeroFilter::new("", &ZeroElisionConfig::default());
  assert_eq!(
    filter.decide(
      &mock_metric(MetricValue::Simple(0_f64), None),
      &Some(MetricValue::Simple(1_f64)),
      None
    ),
    MetricFilterDecision::NotCovered
  );
}

#[test]
fn decide_nonzero() {
  let filter = ZeroFilter::new("", &ZeroElisionConfig::default());
  assert_eq!(
    filter.decide(
      &mock_metric(MetricValue::Simple(5_f64), None),
      &Some(MetricValue::Simple(1_f64)),
      None
    ),
    MetricFilterDecision::NotCovered
  );
}
