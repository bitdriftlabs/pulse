// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::*;
use crate::filters::filter::SimpleMetricFilter;
use crate::filters::zero::ZeroFilter;
use crate::pipeline::processor::elision::SkipDecision::{DoNotOutput, Output};
use crate::protos::carbon::parse;
use crate::protos::metric::{CounterType, DownstreamId, MetricId, MetricSource};
use bd_server_stats::stats::Collector;
use bytes::Bytes;
use elision_config::ZeroElisionConfig;
use elision_config::emit_config::ConsistentEveryPeriod;
use matches::assert_matches;
use pulse_protobuf::protos::pulse::config::processor::v1::elision::elision_config;
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

fn make_emit_n_periods(periods: u32) -> EmitConfig {
  EmitConfig {
    emit_type: Emit_type::ConsistentEveryPeriod(ConsistentEveryPeriod {
      periods,
      ..Default::default()
    })
    .into(),
    ..Default::default()
  }
}

fn mock_filter(decision: MetricFilterDecision, name: &str) -> DynamicMetricFilter {
  Arc::new(SimpleMetricFilter::new(decision, name))
}

fn mock_metric(
  name: bytes::Bytes,
  metric_type: Option<MetricType>,
  value: f64,
  timestamp: u64,
) -> ParsedMetric {
  ParsedMetric::new(
    Metric::new(
      MetricId::new(name, metric_type, vec![], false).unwrap(),
      None,
      timestamp,
      MetricValue::Simple(value),
    ),
    MetricSource::PromRemoteWrite,
    Instant::now(),
    DownstreamId::LocalOrigin,
  )
}

fn mock_parsed_metric(
  name: bytes::Bytes,
  value: f64,
  timestamp: u64,
  metric_cache: &Arc<MetricCache>,
) -> ParsedMetric {
  let mut parsed_metric = mock_metric(name, None, value, timestamp);
  parsed_metric.initialize_cache(metric_cache);
  parsed_metric
}

#[test]
fn state_update_out_of_order() {
  let mut s = State::new(0, 0);
  let metric_filters: Vec<DynamicMetricFilter> = vec![];
  assert_eq!(
    s.update(
      &mock_metric(Bytes::default(), None, 0_f64, 3),
      &metric_filters
    )
    .0,
    FilterDecision::NotCovered
  );
  assert_eq!(
    s.update(
      &mock_metric(Bytes::default(), None, 0_f64, 2),
      &metric_filters
    )
    .0,
    FilterDecision::NotCovered
  );
  assert_eq!(
    s.update(
      &mock_metric(Bytes::default(), None, 0_f64, 4),
      &metric_filters
    )
    .0,
    FilterDecision::NotCovered
  );
}

#[test]
fn state_with_metric_type() {
  let mut s = State::new(0, 0);
  let metric_filters: Vec<DynamicMetricFilter> =
    vec![Arc::new(ZeroFilter::new("", &ZeroElisionConfig::default()))];
  assert_eq!(
    s.update(
      &mock_metric(
        Bytes::default(),
        Some(MetricType::Counter(CounterType::Absolute)),
        0_f64,
        1
      ),
      &metric_filters
    )
    .0,
    FilterDecision::NotCovered
  );
  assert_eq!(
    s.update(
      &mock_metric(
        Bytes::default(),
        Some(MetricType::Counter(CounterType::Absolute)),
        0_f64,
        2
      ),
      &metric_filters
    )
    .0,
    FilterDecision::PeriodicOutput(PeriodicOutputState::new(1, 2))
  );
  assert_eq!(
    s.update(
      &mock_metric(
        Bytes::default(),
        Some(MetricType::Counter(CounterType::Absolute)),
        101_f64,
        3
      ),
      &metric_filters
    )
    .0,
    FilterDecision::NotCovered
  );
}

#[test]
fn state_update_metric_filter() {
  let mut s = State::new(0, 0);
  let metric_filters: &[DynamicMetricFilter] =
    &[mock_filter(MetricFilterDecision::Fail, "mock_filter")];
  assert_eq!(
    s.update(
      &mock_metric(Bytes::default(), None, 0_f64, 1),
      metric_filters
    ),
    (
      FilterDecision::PeriodicOutput(PeriodicOutputState::new(1, 1)),
      Some(0)
    ),
  );
  assert_eq!(
    s.update(
      &mock_metric(Bytes::default(), None, 1_f64, 5),
      metric_filters
    ),
    (
      FilterDecision::PeriodicOutput(PeriodicOutputState::new(2, 5)),
      Some(0)
    )
  );
}

#[test]
fn multiple_metric_filters() {
  let mut s = State::new(0, 0);
  assert_eq!(
    s.update(
      &mock_metric(Bytes::default(), None, 0_f64, 1),
      &[
        mock_filter(MetricFilterDecision::NotCovered, ""),
        mock_filter(MetricFilterDecision::Fail, "")
      ]
    ),
    (
      FilterDecision::PeriodicOutput(PeriodicOutputState::new(1, 1)),
      Some(1)
    )
  );
  assert_eq!(
    s.update(
      &mock_metric(Bytes::default(), None, 0_f64, 2),
      &[
        mock_filter(MetricFilterDecision::Pass, ""),
        mock_filter(MetricFilterDecision::Fail, "")
      ]
    ),
    (FilterDecision::Pass, Some(0))
  );
}

#[test]
fn explicit_skip_decider_with_n_secs() {
  let d = SkipDecider {
    emit: make_emit_n_periods(10),
  };
  let mut ts = 0;
  let mut up = || -> SkipDecision {
    let result = d.decide(
      mock_metric(Bytes::default(), None, 10_f64, ts).metric(),
      FilterDecision::PeriodicOutput(PeriodicOutputState {
        successive_fails: 0,
        seconds_since_last_emitted: 0,
      }),
      None,
    );
    ts += 60;
    result
  };

  assert_matches!(up(), DoNotOutput);
  assert_matches!(up(), DoNotOutput);
  assert_matches!(up(), DoNotOutput);
  assert_matches!(up(), DoNotOutput);
  assert_matches!(up(), DoNotOutput);
  assert_matches!(up(), DoNotOutput);
  assert_matches!(up(), DoNotOutput);
  assert_matches!(up(), DoNotOutput);
  assert_matches!(up(), Output);
  assert_matches!(up(), DoNotOutput);
  assert_matches!(up(), DoNotOutput);
  assert_matches!(up(), DoNotOutput);
  assert_matches!(up(), DoNotOutput);
  assert_matches!(up(), DoNotOutput);
  assert_matches!(up(), DoNotOutput);
  assert_matches!(up(), DoNotOutput);
  assert_matches!(up(), DoNotOutput);
  assert_matches!(up(), DoNotOutput);
  assert_matches!(up(), Output);
}

#[test]
fn explicit_skip_decider_with_metric_filter() {
  let d = SkipDecider {
    emit: make_emit_ratio(0.25),
  };
  let mut s = State::new(0, 0);
  let mut ts = 1;
  let mut up = |v: MetricFilterDecision| -> SkipDecision {
    ts += 1;
    let metric_filters: Vec<DynamicMetricFilter> = vec![mock_filter(v, "")];
    d.decide(
      mock_metric(Bytes::default(), None, 10_f64, ts).metric(),
      s.update(
        &mock_metric(Bytes::default(), None, 10_f64, ts),
        &metric_filters,
      )
      .0,
      None,
    )
  };

  assert_matches!(up(MetricFilterDecision::Pass), Output);
  assert_matches!(up(MetricFilterDecision::Fail), DoNotOutput);
  assert_matches!(up(MetricFilterDecision::Fail), DoNotOutput);
  assert_matches!(up(MetricFilterDecision::Fail), DoNotOutput);
  assert_matches!(up(MetricFilterDecision::Fail), Output);
  assert_matches!(up(MetricFilterDecision::Fail), DoNotOutput);
}

#[test]
fn explicit_skip_decider_emit_interval() {
  let d = SkipDecider {
    emit: make_emit_interval(60),
  };
  let mut s = State::new(0, 0);
  let mut ts = 0;
  let mut up = |v: MetricFilterDecision| -> SkipDecision {
    ts += 20;
    let metric_filters: Vec<DynamicMetricFilter> = vec![mock_filter(v, "")];
    let decision = d.decide(
      mock_metric(Bytes::default(), None, 0_f64, ts).metric(),
      s.update(
        &mock_metric(Bytes::default(), None, 0_f64, ts),
        &metric_filters,
      )
      .0,
      None,
    );
    s.update_last_elided_or_emitted(&decision, ts);
    decision
  };

  assert_matches!(up(MetricFilterDecision::Pass), Output);
  assert_matches!(up(MetricFilterDecision::Pass), Output);
  assert_matches!(up(MetricFilterDecision::Fail), DoNotOutput);
  assert_matches!(up(MetricFilterDecision::Fail), DoNotOutput);
  assert_matches!(up(MetricFilterDecision::Fail), Output);
  assert_matches!(up(MetricFilterDecision::Fail), DoNotOutput);
  assert_matches!(up(MetricFilterDecision::Pass), Output);
}

#[test]
fn state_skip_decider() {
  let d = SkipDecider {
    emit: make_emit_ratio(0.1),
  };
  let mut s = State::new(0, 0);
  let mut outputs = 0;
  for t in 0 .. 100 {
    match d.decide(
      mock_metric(Bytes::default(), None, 0_f64, t).metric(),
      s.update(
        &mock_metric(Bytes::default(), None, 0_f64, t),
        &[Arc::new(ZeroFilter::new("", &ZeroElisionConfig::default()))],
      )
      .0,
      None,
    ) {
      SkipDecision::Output if t % 10 == 0 => {
        outputs += 1;
      },
      SkipDecision::DoNotOutput if t % 10 != 0 => (),
      n => {
        panic!("skip decision was not correct for t = {t} decision {n:?}");
      },
    }
  }
  assert_eq!(outputs, 10);
}

// The initialization step moves the state machine forward by 5 states,
// giving us one more emission from the non-initialized case
#[test]
fn state_skip_decider_init() {
  let d = SkipDecider {
    emit: make_emit_ratio(0.1),
  };
  let mut s = State::new(0, 5);
  let mut outputs = 0;
  for t in 0 .. 100 {
    match d.decide(
      mock_metric(Bytes::default(), None, 0_f64, t).metric(),
      s.update(
        &mock_metric(Bytes::default(), None, 0_f64, t),
        &[
          Arc::new(ZeroFilter::new("", &ZeroElisionConfig::default())),
          Arc::new(SimpleMetricFilter::new(
            MetricFilterDecision::NotCovered,
            "not_covered",
          )),
        ],
      )
      .0,
      None,
    ) {
      SkipDecision::Output if t == 0 => {
        outputs += 1;
      },
      SkipDecision::Output if (t + 5) % 10 == 0 => {
        outputs += 1;
      },
      SkipDecision::DoNotOutput if (t + 5) % 10 != 0 => {},
      n => {
        panic!("skip decision was not correct for t = {t} decision {n:?}");
      },
    }
  }
  assert_eq!(outputs, 11);
}

#[test]
fn overflow() {
  let scope = Collector::default().scope("test");
  let metric_cache = MetricCache::new(&scope.scope("metric_cache"), Some(1));
  let map = ElisionState::new(metric_cache.clone(), &scope.scope("elision"));
  let metric_filters: Vec<DynamicMetricFilter> =
    vec![Arc::new(ZeroFilter::new("", &ZeroElisionConfig::default()))];
  let skip_decider = SkipDecider::new(make_emit_interval(1));
  assert_eq!(
    map.update_and_decide(
      &mock_parsed_metric("hello".to_string().into(), 1_f64, 0, &metric_cache),
      &metric_filters,
      &skip_decider,
      None,
    ),
    (SkipDecision::Output, None)
  );
  assert_eq!(
    map.update_and_decide(
      &mock_parsed_metric("world".to_string().into(), 1_f64, 0, &metric_cache),
      &metric_filters,
      &skip_decider,
      None,
    ),
    (SkipDecision::Output, None)
  );
}

// Test the outermost elision structure, with one key
#[test]
fn elision_map() {
  let carbon: bytes::Bytes = "hello 1.2 1 a=b".into();
  let metric = parse(&carbon).expect("valid carbon line");
  let name = metric.get_id().name().clone();
  let metric_filters: Vec<DynamicMetricFilter> =
    vec![Arc::new(ZeroFilter::new("", &ZeroElisionConfig::default()))];
  // Emit every second, we test the skip decider elsewhere
  let skip_decider = SkipDecider::new(make_emit_interval(1));

  let scope = Collector::default().scope("test");
  let metric_cache = MetricCache::new(&scope, None);
  let map = ElisionState::new(metric_cache.clone(), &scope.scope("elision"));
  assert_eq!(
    map.update_and_decide(
      &mock_parsed_metric(name.clone(), 1_f64, 0, &metric_cache),
      &metric_filters,
      &skip_decider,
      None,
    ),
    (SkipDecision::Output, None)
  );
  // Non-zero values shouldn't be covered
  for ts in 1 .. 100 {
    assert_eq!(
      map.update_and_decide(
        &mock_parsed_metric(name.clone(), 1_f64, ts, &metric_cache),
        &metric_filters,
        &skip_decider,
        None,
      ),
      (SkipDecision::Output, None)
    );
  }
  // Out of order timestamps shouldn't be covered
  assert_eq!(
    map.update_and_decide(
      &mock_parsed_metric(name.clone(), 0_f64, 100, &metric_cache),
      &metric_filters,
      &skip_decider,
      None,
    ),
    (SkipDecision::Output, None)
  );
  // Check elision behavior for zero values
  for ts in 101 .. 200 {
    match map
      .update_and_decide(
        &mock_parsed_metric(name.clone(), 0_f64, ts, &metric_cache),
        &metric_filters,
        &skip_decider,
        None,
      )
      .0
    {
      SkipDecision::Output => (),
      n @ SkipDecision::DoNotOutput => panic!("invalid state output {n:?}"),
    }
  }
  let r = map
    .update_and_decide(
      &mock_parsed_metric(name, 0_f64, 200, &metric_cache),
      &metric_filters,
      &skip_decider,
      None,
    )
    .0;
  assert_eq!(r, SkipDecision::Output);
}
