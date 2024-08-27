// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::pipeline::processor::aggregation::test::HelperBuilder;
use bd_time::TimeDurationExt;
use time::ext::NumericalDuration;

// Make sure negative counters don't crash.
#[tokio::test(start_paused = true)]
async fn negative_counters() {
  let mut helper = HelperBuilder::default().build().await;

  helper
    .recv(vec![helper.make_abs_counter("hello", -100.0, "127.0.0.1")])
    .await;
  helper.expect_dispatch(vec![]);
  61.seconds().sleep().await;

  helper
    .recv(vec![helper.make_abs_counter("hello", -100.0, "127.0.0.1")])
    .await;
  helper.expect_dispatch(vec![]);
  61.seconds().sleep().await;
}

// Make sure that absolute counters are only considered new after generation 2.
#[tokio::test(start_paused = true)]
async fn new_metric_after_gen_2() {
  let mut helper = HelperBuilder::default().build().await;

  // First flush has nothing.
  helper.expect_dispatch(vec![]);
  61.seconds().sleep().await;

  // Second flush has the first sample, should still emit nothing.
  helper
    .recv(vec![helper.make_abs_counter("hello", 100.0, "127.0.0.1")])
    .await;
  helper.expect_dispatch(vec![]);
  61.seconds().sleep().await;

  // Third flush should emit.
  helper
    .recv(vec![helper.make_abs_counter("hello", 101.0, "127.0.0.1")])
    .await;
  helper.expect_dispatch(vec![helper.expect_acounter("counters.hello", 1.0)]);
  61.seconds().sleep().await;
}

// Various absolute counter scenarios from a single origin.
#[tokio::test(start_paused = true)]
async fn absolute_single_origin() {
  let mut helper = HelperBuilder::default().build().await;

  // First flush emits nothing as we just started up and we only have one sample.
  helper
    .recv(vec![helper.make_abs_counter("hello", 100.0, "127.0.0.1")])
    .await;
  helper.expect_dispatch(vec![]);
  61.seconds().sleep().await;

  // Second flush is able to create the delta via the previous value.
  helper
    .recv(vec![helper.make_abs_counter("hello", 101.0, "127.0.0.1")])
    .await;
  helper.expect_dispatch(vec![helper.expect_acounter("counters.hello", 1.0)]);
  61.seconds().sleep().await;

  // Third flush is able to create the delta from within the interval.
  helper
    .recv(vec![helper.make_abs_counter("hello", 101.0, "127.0.0.1")])
    .await;
  helper
    .recv(vec![helper.make_abs_counter("hello", 102.0, "127.0.0.1")])
    .await;
  helper.expect_dispatch(vec![helper.expect_acounter("counters.hello", 2.0)]);
  61.seconds().sleep().await;

  // Fourth flush has multiple delta updates within the interval.
  helper
    .recv(vec![helper.make_abs_counter("hello", 102.0, "127.0.0.1")])
    .await;
  helper
    .recv(vec![helper.make_abs_counter("hello", 102.0, "127.0.0.1")])
    .await;
  helper
    .recv(vec![helper.make_abs_counter("hello", 200.0, "127.0.0.1")])
    .await;
  helper.expect_dispatch(vec![helper.expect_acounter("counters.hello", 100.0)]);
  61.seconds().sleep().await;
}

// Verify configuration to emit absolute counters as delta rates.
#[tokio::test(start_paused = true)]
async fn absolute_emit_as_delta() {
  let mut helper = HelperBuilder::default()
    .emit_absolute_counters_as_delta_rate()
    .build()
    .await;
  helper
    .recv(vec![helper.make_abs_counter("hello", 100.0, "127.0.0.1")])
    .await;
  helper.expect_dispatch(vec![]);
  61.seconds().sleep().await;
  helper
    .recv(vec![helper.make_abs_counter("hello", 101.0, "127.0.0.1")])
    .await;
  helper.expect_dispatch(vec![
    helper.expect_dcounter("counters.hello", 0.016_666_666_666_666_666)
  ]);
  61.seconds().sleep().await;
}

// Verify different types of resets both within and across intervals.
#[tokio::test(start_paused = true)]
async fn absolute_single_origin_reset_within_interval() {
  let mut helper = HelperBuilder::default().build().await;

  // During startup we get 2 samples, and one of them is a reset.
  helper
    .recv(vec![helper.make_abs_counter("hello", 100.0, "127.0.0.1")])
    .await;
  helper.expect_dispatch(vec![]);
  61.seconds().sleep().await;
  helper
    .recv(vec![helper.make_abs_counter("hello", 1.0, "127.0.0.1")])
    .await;
  helper.expect_dispatch(vec![helper.expect_acounter("counters.hello", 1.0)]);
  61.seconds().sleep().await;

  // Send a single sample within the interval.
  helper
    .recv(vec![helper.make_abs_counter("hello", 100.0, "127.0.0.1")])
    .await;
  helper.expect_dispatch(vec![helper.expect_acounter("counters.hello", 100.0)]);
  61.seconds().sleep().await;

  // Reset across the interval.
  helper
    .recv(vec![helper.make_abs_counter("hello", 5.0, "127.0.0.1")])
    .await;
  helper.expect_dispatch(vec![helper.expect_acounter("counters.hello", 105.0)]);
  61.seconds().sleep().await;
}

// Verify that we drop absolute counters < 0.
#[tokio::test(start_paused = true)]
async fn absolute_counter_below_zero() {
  let mut helper = HelperBuilder::default().build().await;
  helper
    .recv(vec![helper.make_abs_counter("hello", -4.0, "127.0.0.2")])
    .await;
  helper
    .recv(vec![helper.make_abs_counter("hello", -5.0, "127.0.0.2")])
    .await;
  helper.expect_dispatch(vec![]);
  61.seconds().sleep().await;
}

// Verify functionality with multiple origins.
#[tokio::test(start_paused = true)]
async fn absolute_multiple_origin() {
  let mut helper = HelperBuilder::default().build().await;

  // We don't get multiple samples in the first window so we can't emit.
  helper
    .recv(vec![
      helper.make_abs_counter("hello", 100.0, "127.0.0.1"),
      helper.make_abs_counter("hello", 1000.0, "127.0.0.2"),
    ])
    .await;
  helper.expect_dispatch(vec![]);
  61.seconds().sleep().await;

  // We get samples in the 2nd window so we can emit.
  helper
    .recv(vec![
      helper.make_abs_counter("hello", 105.0, "127.0.0.1"),
      helper.make_abs_counter("hello", 1003.0, "127.0.0.2"),
    ])
    .await;
  helper.expect_dispatch(vec![helper.expect_acounter("counters.hello", 8.0)]);
  61.seconds().sleep().await;

  // One source drops. We can still emit.
  helper
    .recv(vec![helper.make_abs_counter("hello", 1004.0, "127.0.0.2")])
    .await;
  helper.expect_dispatch(vec![helper.expect_acounter("counters.hello", 9.0)]);
  61.seconds().sleep().await;

  // A new source gets added, we should be able to emit, because we assume it is a new process.
  helper
    .recv(vec![
      helper.make_abs_counter("hello", 5.0, "127.0.0.1"),
      helper.make_abs_counter("hello", 1004.0, "127.0.0.2"),
    ])
    .await;
  helper.expect_dispatch(vec![helper.expect_acounter("counters.hello", 14.0)]);
  61.seconds().sleep().await;
}
