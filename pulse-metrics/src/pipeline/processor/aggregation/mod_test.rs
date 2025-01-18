// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::*;
use crate::pipeline::processor::aggregation::test::HelperBuilder;
use crate::pipeline::time::TestTimeProvider;
use crate::protos::metric::{DownstreamId, MetricSource, SummaryBucket, SummaryData};
use crate::protos::prom::prom_stale_marker;
use bd_test_helpers::make_mut;
use bd_time::TimeDurationExt;
use http_body_util::BodyExt;
use mod_test::test::TimerType;
use pretty_assertions::assert_eq;
use prometheus::labels;
use std::sync::atomic::AtomicI64;
use std::time::Duration;
use time::ext::NumericalDuration;

// TODO(mattklein123): Test tags.

fn make_time_provider(time: i64) -> TestTimeProvider {
  TestTimeProvider {
    time: Arc::new(AtomicI64::new(time)),
  }
}

#[test]
fn flush_interval() {
  assert_eq!(1.minutes(), next_flush_interval(&make_time_provider(0), 60));
  assert_eq!(
    1.minutes(),
    next_flush_interval(&make_time_provider(120), 60)
  );
  assert_eq!(
    Duration::from_secs(59),
    next_flush_interval(&make_time_provider(121), 60)
  );
  assert_eq!(
    1.seconds(),
    next_flush_interval(&make_time_provider(179), 60)
  );
  assert_eq!(
    Duration::from_secs(2),
    next_flush_interval(&make_time_provider(123), 5)
  );
}

#[tokio::test(start_paused = true)]
async fn missed_sample() {
  let mut helper = HelperBuilder::default().build().await;
  helper
    .recv(vec![helper.make_metric_ex(
      "hello",
      Some(MetricType::Counter(CounterType::Delta)),
      None,
      MetricValue::Simple(10.0),
      MetricSource::PromRemoteWrite,
      DownstreamId::LocalOrigin,
    )])
    .await;
  helper.expect_dispatch(vec![helper.expect_dcounter("counters.hello", 10.0)]);
  61.seconds().sleep().await;

  helper.expect_dispatch(vec![]);
  61.seconds().sleep().await;

  helper
    .recv(vec![helper.make_metric_ex(
      "hello",
      Some(MetricType::Counter(CounterType::Delta)),
      None,
      MetricValue::Simple(10.0),
      MetricSource::PromRemoteWrite,
      DownstreamId::LocalOrigin,
    )])
    .await;
  helper.expect_dispatch(vec![helper.expect_dcounter("counters.hello", 10.0)]);
  61.seconds().sleep().await;
}

#[tokio::test(start_paused = true)]
async fn new_metric_during_flush_race() {
  let mut helper = HelperBuilder::default().build().await;
  helper.thread_synchronizer().wait_on("do_flush").await;

  // Start the flush and wait for the snapshot swap to start.
  61.seconds().sleep().await;
  helper.thread_synchronizer().barrier_on("do_flush").await;

  // Now add a new metric. This should not get flushed during the previous flush window.
  helper
    .recv(vec![helper.make_metric_ex(
      "hello",
      Some(MetricType::Counter(CounterType::Delta)),
      None,
      MetricValue::Simple(10.0),
      MetricSource::PromRemoteWrite,
      DownstreamId::LocalOrigin,
    )])
    .await;
  helper.expect_dispatch(vec![]);
  helper.thread_synchronizer().signal("do_flush").await;
  1.seconds().sleep().await;

  // Now do a another flush and we should get the metric.
  helper.expect_dispatch(vec![helper.expect_dcounter("counters.hello", 10.0)]);
  1.minutes().sleep().await;
}

#[tokio::test(start_paused = true)]
async fn missed_samples_catch_up() {
  let mut helper = HelperBuilder::default().build().await;
  helper
    .recv(vec![helper.make_metric_ex(
      "hello",
      Some(MetricType::Counter(CounterType::Delta)),
      None,
      MetricValue::Simple(1.0),
      MetricSource::PromRemoteWrite,
      DownstreamId::LocalOrigin,
    )])
    .await;

  helper.expect_dispatch(vec![helper.expect_dcounter("counters.hello", 1.0)]);
  61.seconds().sleep().await;
  helper.expect_dispatch(vec![]);
  61.seconds().sleep().await;
  helper.expect_dispatch(vec![]);
  61.seconds().sleep().await;

  helper
    .recv(vec![helper.make_metric_ex(
      "hello",
      Some(MetricType::Counter(CounterType::Delta)),
      None,
      MetricValue::Simple(1.0),
      MetricSource::PromRemoteWrite,
      DownstreamId::LocalOrigin,
    )])
    .await;
  helper
    .recv(vec![helper.make_metric_ex(
      "hello",
      Some(MetricType::Counter(CounterType::Delta)),
      None,
      MetricValue::Simple(1.0),
      MetricSource::PromRemoteWrite,
      DownstreamId::LocalOrigin,
    )])
    .await;

  helper.expect_dispatch(vec![helper.expect_dcounter("counters.hello", 2.0)]);
  61.seconds().sleep().await;
  helper.expect_dispatch(vec![]);
  61.seconds().sleep().await;
}

#[tokio::test(start_paused = true)]
async fn flush_race() {
  let mut helper = HelperBuilder::default().build().await;
  helper.thread_synchronizer().wait_on("do_flush").await;
  helper
    .recv(vec![helper.make_metric_ex(
      "hello",
      Some(MetricType::Counter(CounterType::Delta)),
      None,
      MetricValue::Simple(10.0),
      MetricSource::PromRemoteWrite,
      DownstreamId::LocalOrigin,
    )])
    .await;

  // Start the flush and wait for the snapshot swap to start.
  61.seconds().sleep().await;
  helper.thread_synchronizer().barrier_on("do_flush").await;

  // Send in a new sample when the previous generation has not yet been completely flushed.
  helper
    .recv(vec![helper.make_metric_ex(
      "hello",
      Some(MetricType::Counter(CounterType::Delta)),
      None,
      MetricValue::Simple(15.0),
      MetricSource::PromRemoteWrite,
      DownstreamId::LocalOrigin,
    )])
    .await;

  // Finish the flush.
  helper.expect_dispatch(vec![helper.expect_dcounter("counters.hello", 10.0)]);
  helper.thread_synchronizer().signal("do_flush").await;
  1.seconds().sleep().await;

  // Send some more data which should use aggregation vs next_aggregation.
  helper
    .recv(vec![helper.make_metric_ex(
      "hello",
      Some(MetricType::Counter(CounterType::Delta)),
      None,
      MetricValue::Simple(5.0),
      MetricSource::PromRemoteWrite,
      DownstreamId::LocalOrigin,
    )])
    .await;

  // Do another flush and we should get the result use both samples.
  helper.expect_dispatch(vec![helper.expect_dcounter("counters.hello", 20.0)]);
  1.minutes().sleep().await;

  // Make sure the entire process works correctly for new metrics received after the first
  // generation.
  helper
    .recv(vec![
      helper.make_metric_ex(
        "hello",
        Some(MetricType::Counter(CounterType::Delta)),
        None,
        MetricValue::Simple(1.0),
        MetricSource::PromRemoteWrite,
        DownstreamId::LocalOrigin,
      ),
      helper.make_metric_ex(
        "world",
        Some(MetricType::Counter(CounterType::Delta)),
        None,
        MetricValue::Simple(1.0),
        MetricSource::PromRemoteWrite,
        DownstreamId::LocalOrigin,
      ),
    ])
    .await;
  helper.thread_synchronizer().wait_on("do_flush").await;
  61.seconds().sleep().await;
  helper.thread_synchronizer().barrier_on("do_flush").await;
  helper
    .recv(vec![helper.make_metric_ex(
      "world",
      Some(MetricType::Counter(CounterType::Delta)),
      None,
      MetricValue::Simple(1.0),
      MetricSource::PromRemoteWrite,
      DownstreamId::LocalOrigin,
    )])
    .await;
  helper.expect_dispatch(vec![
    helper.expect_dcounter("counters.hello", 1.0),
    helper.expect_dcounter("counters.world", 1.0),
  ]);
  helper.thread_synchronizer().signal("do_flush").await;
  1.seconds().sleep().await;
  helper.expect_dispatch(vec![helper.expect_dcounter("counters.world", 1.0)]);
  1.minutes().sleep().await;
}

#[tokio::test(start_paused = true)]
async fn unsupported() {
  let mut helper = HelperBuilder::default().build().await;
  helper
    .recv(vec![helper.make_metric_ex(
      "hello",
      None,
      None,
      MetricValue::Simple(1.0),
      MetricSource::PromRemoteWrite,
      DownstreamId::LocalOrigin,
    )])
    .await;
  helper.expect_dispatch(vec![]);
  61.seconds().sleep().await;
}

#[tokio::test(start_paused = true)]
async fn bulk_timers() {
  let mut helper = HelperBuilder::default()
    .timer_type(TimerType::Reservoir)
    .build()
    .await;

  helper
    .recv(vec![helper.make_metric(
      "hello",
      MetricType::BulkTimer,
      MetricValue::BulkTimer((0 .. 200u64).map(|i| i as f64).collect()),
    )])
    .await;

  make_mut(&mut helper.helper.dispatcher)
    .expect_send()
    .times(1)
    .return_once(move |samples| {
      assert_eq!(100, samples.len());
      assert_eq!(0.5, samples[0].metric().sample_rate.unwrap());
    });
  61.seconds().sleep().await;
}

#[tokio::test(start_paused = true)]
async fn reservoir_timers() {
  let mut helper = HelperBuilder::default()
    .timer_type(TimerType::Reservoir)
    .build()
    .await;

  for i in 0 .. 200u64 {
    helper
      .recv(vec![helper.make_metric(
        "hello",
        MetricType::Timer,
        MetricValue::Simple(i as f64),
      )])
      .await;
  }

  make_mut(&mut helper.helper.dispatcher)
    .expect_send()
    .times(1)
    .return_once(move |samples| {
      assert_eq!(100, samples.len());
      assert_eq!(0.5, samples[0].metric().sample_rate.unwrap());
    });
  61.seconds().sleep().await;

  for i in 0 .. 50u64 {
    helper
      .recv(vec![helper.make_metric(
        "hello",
        MetricType::Timer,
        MetricValue::Simple(i as f64),
      )])
      .await;
  }

  make_mut(&mut helper.helper.dispatcher)
    .expect_send()
    .times(1)
    .return_once(move |samples| {
      assert_eq!(50, samples.len());
      assert_eq!(1.0, samples[0].metric().sample_rate.unwrap());
    });
  61.seconds().sleep().await;
}

#[tokio::test(start_paused = true)]
async fn timers() {
  let mut helper = HelperBuilder::default()
    .timer_type(TimerType::QuantileExtended)
    .enable_last_aggregation_admin_endpoint()
    .enable_emit_prometheus_stale_markers()
    .build()
    .await;
  for i in 0 .. 100u64 {
    helper
      .recv(vec![helper.make_metric(
        "hello",
        MetricType::Timer,
        MetricValue::Simple(i as f64),
      )])
      .await;
  }

  let response = String::from_utf8(
    helper
      .helper
      .admin
      .run("/last_aggregation")
      .await
      .into_body()
      .collect()
      .await
      .unwrap()
      .to_bytes()
      .to_vec(),
  )
  .unwrap();
  assert_eq!("", response);

  let expected = vec![
    helper.expect_gauge("timers.hello.mean", 49.5),
    helper.expect_gauge("timers.hello.lower", 0.0),
    helper.expect_gauge("timers.hello.upper", 99.0),
    helper.expect_dcounter("timers.hello.count", 100.0),
    helper.expect_dcounter("timers.hello.rate", 82.5),
    helper.expect_gauge("timers.hello.p50", 49.0),
    helper.expect_gauge("timers.hello.p95", 94.0),
    helper.expect_gauge("timers.hello.p99", 98.0),
    helper.expect_gauge("timers.hello.p999", 99.0),
  ];
  helper.expect_dispatch(expected);
  61.seconds().sleep().await;

  let response = String::from_utf8(
    helper
      .helper
      .admin
      .run("/last_aggregation")
      .await
      .into_body()
      .collect()
      .await
      .unwrap()
      .to_bytes()
      .to_vec(),
  )
  .unwrap();
  assert_eq!(
    r"dumping filter: test
timers.hello.mean()
timers.hello.lower()
timers.hello.upper()
timers.hello.count()
timers.hello.rate()
timers.hello.p50()
timers.hello.p95()
timers.hello.p99()
timers.hello.p999()
",
    response
  );

  let expected = vec![
    helper.expect_gauge("timers.hello.mean", prom_stale_marker()),
    helper.expect_gauge("timers.hello.lower", prom_stale_marker()),
    helper.expect_gauge("timers.hello.upper", prom_stale_marker()),
    helper.expect_dcounter("timers.hello.count", prom_stale_marker()),
    helper.expect_dcounter("timers.hello.rate", prom_stale_marker()),
    helper.expect_gauge("timers.hello.p50", prom_stale_marker()),
    helper.expect_gauge("timers.hello.p95", prom_stale_marker()),
    helper.expect_gauge("timers.hello.p99", prom_stale_marker()),
    helper.expect_gauge("timers.hello.p999", prom_stale_marker()),
  ];
  helper.expect_dispatch(expected);
  61.seconds().sleep().await;

  helper
    .helper
    .stats_helper
    .assert_counter_eq(9, "processor:stale_markers", &labels! {});
}

#[tokio::test(start_paused = true)]
async fn summaries() {
  let mut helper = HelperBuilder::default()
    .enable_emit_prometheus_stale_markers()
    .build()
    .await;
  helper
    .recv(vec![helper.make_metric(
      "hello",
      MetricType::Summary,
      MetricValue::Summary(SummaryData {
        quantiles: vec![
          SummaryBucket {
            quantile: 0.5,
            value: 20.0,
          },
          SummaryBucket {
            quantile: 0.9,
            value: 30.0,
          },
        ],
        sample_count: 40.0,
        sample_sum: 150.0,
      }),
    )])
    .await;
  helper
    .recv(vec![helper.make_metric(
      "hello",
      MetricType::Summary,
      MetricValue::Summary(SummaryData {
        quantiles: vec![
          SummaryBucket {
            quantile: 0.5,
            value: 30.0,
          },
          SummaryBucket {
            quantile: 0.9,
            value: 40.0,
          },
        ],
        sample_count: 20.0,
        sample_sum: 125.0,
      }),
    )])
    .await;
  let expected = vec![helper.expected_metric(
    "hello",
    MetricValue::Summary(SummaryData {
      quantiles: vec![
        SummaryBucket {
          quantile: 0.5,
          value: 30.0,
        },
        SummaryBucket {
          quantile: 0.9,
          value: 40.0,
        },
      ],
      sample_count: 60.0,
      sample_sum: 275.0,
    }),
    MetricType::Summary,
  )];
  helper.expect_dispatch(expected);
  61.seconds().sleep().await;
  let expected = vec![helper.expected_metric(
    "hello",
    MetricValue::Summary(SummaryData {
      quantiles: vec![
        SummaryBucket {
          quantile: 0.5,
          value: prom_stale_marker(),
        },
        SummaryBucket {
          quantile: 0.9,
          value: prom_stale_marker(),
        },
      ],
      sample_count: prom_stale_marker(),
      sample_sum: prom_stale_marker(),
    }),
    MetricType::Summary,
  )];
  helper.expect_dispatch(expected);
  61.seconds().sleep().await;
}

#[tokio::test(start_paused = true)]
async fn counters() {
  let mut helper = HelperBuilder::default().extended_counters().build().await;
  helper
    .recv(vec![helper.make_metric(
      "hello",
      MetricType::Counter(CounterType::Delta),
      MetricValue::Simple(5.0),
    )])
    .await;
  helper
    .recv(vec![helper.make_metric(
      "hello",
      MetricType::Counter(CounterType::Delta),
      MetricValue::Simple(10.0),
    )])
    .await;
  helper
    .recv(vec![helper.make_metric(
      "hello",
      MetricType::Counter(CounterType::Delta),
      MetricValue::Simple(0.0),
    )])
    .await;
  let expected = vec![
    helper.expect_dcounter("counters.hello.count", 2.0),
    helper.expect_dcounter("counters.hello.sum", 15.0),
    helper.expect_gauge("counters.hello.lower", 5.0),
    helper.expect_gauge("counters.hello.upper", 10.0),
    helper.expect_dcounter("counters.hello.rate", 0.25),
  ];
  helper.expect_dispatch(expected);
  61.seconds().sleep().await;
}

#[tokio::test(start_paused = true)]
async fn counter_sampling() {
  let mut helper = HelperBuilder::default().extended_counters().build().await;
  helper
    .recv(vec![helper.make_metric_ex(
      "hello",
      Some(MetricType::Counter(CounterType::Delta)),
      Some(0.25),
      MetricValue::Simple(10.0),
      MetricSource::PromRemoteWrite,
      DownstreamId::LocalOrigin,
    )])
    .await;
  let expected = vec![
    helper.expect_dcounter("counters.hello.count", 4.0),
    helper.expect_dcounter("counters.hello.sum", 40.0),
    helper.expect_gauge("counters.hello.lower", 40.0),
    helper.expect_gauge("counters.hello.upper", 40.0),
    helper.expect_dcounter("counters.hello.rate", 0.666_666_666_666_666_6),
  ];
  helper.expect_dispatch(expected);
  61.seconds().sleep().await;
}

#[tokio::test(start_paused = true)]
async fn gauges() {
  let mut helper = HelperBuilder::default().extended_gauges().build().await;
  helper
    .recv(vec![
      helper.make_metric("hello", MetricType::Gauge, MetricValue::Simple(5.0)),
      helper.make_metric("world", MetricType::DeltaGauge, MetricValue::Simple(5.0)),
      helper.make_metric("foo", MetricType::DirectGauge, MetricValue::Simple(100.0)),
    ])
    .await;
  helper
    .recv(vec![
      helper.make_metric("hello", MetricType::Gauge, MetricValue::Simple(10.0)),
      helper.make_metric("world", MetricType::DeltaGauge, MetricValue::Simple(-5.0)),
      helper.make_metric("foo", MetricType::DirectGauge, MetricValue::Simple(200.0)),
    ])
    .await;
  let expected = vec![
    helper.expect_gauge("gauges.hello", 10.0),
    helper.expect_gauge("gauges.hello.sum", 15.0),
    helper.expect_gauge("gauges.hello.mean", 7.5),
    helper.expect_gauge("gauges.hello.min", 5.0),
    helper.expect_gauge("gauges.hello.max", 10.0),
    helper.expect_gauge("gauges.world", 0.0),
    helper.expect_gauge("gauges.world.sum", 0.0),
    helper.expect_gauge("gauges.world.mean", 0.0),
    helper.expect_gauge("gauges.world.min", -5.0),
    helper.expect_gauge("gauges.world.max", 5.0),
    helper.expect_gauge("gauges.foo", 200.0),
  ];
  helper.expect_dispatch(expected);
  61.seconds().sleep().await;
}

#[tokio::test(start_paused = true)]
async fn flush_jitter() {
  let mut helper = HelperBuilder::default()
    .flush_jitter(15.seconds())
    .extended_gauges()
    .build()
    .await;
  helper
    .recv(vec![helper.make_metric(
      "world",
      MetricType::Gauge,
      MetricValue::Simple(15.0),
    )])
    .await;
  61.seconds().sleep().await;

  let expected = vec![
    helper.expect_gauge("gauges.world", 15.0),
    helper.expect_gauge("gauges.world.sum", 15.0),
    helper.expect_gauge("gauges.world.mean", 15.0),
    helper.expect_gauge("gauges.world.min", 15.0),
    helper.expect_gauge("gauges.world.max", 15.0),
  ];
  helper.expect_dispatch(expected);
  16.seconds().sleep().await;
}

#[tokio::test(start_paused = true)]
async fn shutdown_flush() {
  let mut helper = HelperBuilder::default().extended_gauges().build().await;

  helper
    .recv(vec![helper.make_metric(
      "world",
      MetricType::Gauge,
      MetricValue::Simple(15.0),
    )])
    .await;

  let expected = vec![
    helper.expect_gauge("gauges.world", 15.0),
    helper.expect_gauge("gauges.world.sum", 15.0),
    helper.expect_gauge("gauges.world.mean", 15.0),
    helper.expect_gauge("gauges.world.min", 15.0),
    helper.expect_gauge("gauges.world.max", 15.0),
  ];
  helper.expect_dispatch(expected);
  helper.helper.shutdown_trigger.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn stale_markers() {
  let mut helper = HelperBuilder::default()
    .extended_counters()
    .extended_gauges()
    .enable_emit_prometheus_stale_markers()
    .build()
    .await;

  helper
    .recv(vec![
      helper.make_metric(
        "hello",
        MetricType::Counter(CounterType::Delta),
        MetricValue::Simple(5.0),
      ),
      helper.make_metric("world", MetricType::Gauge, MetricValue::Simple(15.0)),
    ])
    .await;
  let expected = vec![
    helper.expect_dcounter("counters.hello.count", 1.0),
    helper.expect_dcounter("counters.hello.sum", 5.0),
    helper.expect_gauge("counters.hello.lower", 5.0),
    helper.expect_gauge("counters.hello.upper", 5.0),
    helper.expect_dcounter("counters.hello.rate", 0.083_333_333_333_333_33),
    helper.expect_gauge("gauges.world", 15.0),
    helper.expect_gauge("gauges.world.sum", 15.0),
    helper.expect_gauge("gauges.world.mean", 15.0),
    helper.expect_gauge("gauges.world.min", 15.0),
    helper.expect_gauge("gauges.world.max", 15.0),
  ];
  helper.expect_dispatch(expected);
  61.seconds().sleep().await;

  helper
    .recv(vec![helper.make_metric(
      "hello",
      MetricType::Counter(CounterType::Delta),
      MetricValue::Simple(10.0),
    )])
    .await;
  let expected = vec![
    helper.expect_dcounter("counters.hello.count", 1.0),
    helper.expect_dcounter("counters.hello.sum", 10.0),
    helper.expect_gauge("counters.hello.lower", 10.0),
    helper.expect_gauge("counters.hello.upper", 10.0),
    helper.expect_dcounter("counters.hello.rate", 0.166_666_666_666_666_66),
    helper.expect_gauge("gauges.world", prom_stale_marker()),
    helper.expect_gauge("gauges.world.sum", prom_stale_marker()),
    helper.expect_gauge("gauges.world.mean", prom_stale_marker()),
    helper.expect_gauge("gauges.world.min", prom_stale_marker()),
    helper.expect_gauge("gauges.world.max", prom_stale_marker()),
  ];
  helper.expect_dispatch(expected);
  61.seconds().sleep().await;

  let expected = vec![
    helper.expect_dcounter("counters.hello.count", prom_stale_marker()),
    helper.expect_dcounter("counters.hello.sum", prom_stale_marker()),
    helper.expect_gauge("counters.hello.lower", prom_stale_marker()),
    helper.expect_gauge("counters.hello.upper", prom_stale_marker()),
    helper.expect_dcounter("counters.hello.rate", prom_stale_marker()),
  ];
  helper.expect_dispatch(expected);
  61.seconds().sleep().await;
}

#[tokio::test(start_paused = true)]
async fn multiple_snapshots() {
  let mut helper = HelperBuilder::default()
    .extended_counters()
    .extended_gauges()
    .build()
    .await;

  helper
    .recv(vec![
      helper.make_metric(
        "hello",
        MetricType::Counter(CounterType::Delta),
        MetricValue::Simple(5.0),
      ),
      helper.make_metric("world", MetricType::Gauge, MetricValue::Simple(15.0)),
    ])
    .await;
  let expected = vec![
    helper.expect_dcounter("counters.hello.count", 1.0),
    helper.expect_dcounter("counters.hello.sum", 5.0),
    helper.expect_gauge("counters.hello.lower", 5.0),
    helper.expect_gauge("counters.hello.upper", 5.0),
    helper.expect_dcounter("counters.hello.rate", 0.083_333_333_333_333_33),
    helper.expect_gauge("gauges.world", 15.0),
    helper.expect_gauge("gauges.world.sum", 15.0),
    helper.expect_gauge("gauges.world.mean", 15.0),
    helper.expect_gauge("gauges.world.min", 15.0),
    helper.expect_gauge("gauges.world.max", 15.0),
  ];
  helper.expect_dispatch(expected);
  61.seconds().sleep().await;

  helper
    .recv(vec![helper.make_metric(
      "hello",
      MetricType::Counter(CounterType::Delta),
      MetricValue::Simple(10.0),
    )])
    .await;
  let expected = vec![
    helper.expect_dcounter("counters.hello.count", 1.0),
    helper.expect_dcounter("counters.hello.sum", 10.0),
    helper.expect_gauge("counters.hello.lower", 10.0),
    helper.expect_gauge("counters.hello.upper", 10.0),
    helper.expect_dcounter("counters.hello.rate", 0.166_666_666_666_666_66),
  ];
  helper.expect_dispatch(expected);
  61.seconds().sleep().await;
}
