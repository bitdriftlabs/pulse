// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::{SampleDecision, SamplerProcessor, SamplerState};
use crate::pipeline::metric_cache::MetricCache;
use crate::pipeline::processor::PipelineProcessor;
use crate::pipeline::time::TestDurationJitter;
use crate::protos::metric::{
  DownstreamId,
  Metric,
  MetricId,
  MetricSource,
  MetricType,
  MetricValue,
  ParsedMetric,
  TagValue,
};
use crate::test::processor_factory_context_for_test;
use bd_server_stats::stats::Collector;
use bd_test_helpers::make_mut;
use bd_time::{TimeDurationExt, ToProtoDuration};
use bytes::Bytes;
use pulse_protobuf::protos::pulse::config::processor::v1::sampler::SamplerConfig;
use std::sync::Arc;
use time::ext::{NumericalDuration, NumericalStdDuration};
use time::Duration;
use tokio::time::Instant;

fn make_metric(
  name: &'static str,
  received_at: Instant,
  metric_cache: &Arc<MetricCache>,
) -> ParsedMetric {
  let mut parsed_metric = ParsedMetric::new(
    Metric::new(
      MetricId::new(
        Bytes::from_static(name.as_bytes()),
        Some(MetricType::DeltaGauge),
        vec![TagValue {
          tag: Bytes::from_static(b"yummy"),
          value: Bytes::from_static(b"cheese"),
        }],
        false,
      )
      .unwrap(),
      Some(0.2_f64),
      1234,
      MetricValue::Simple(1.5_f64),
    ),
    MetricSource::Carbon(Bytes::from_static(b"original line")),
    received_at.into(),
    DownstreamId::LocalOrigin,
  );
  parsed_metric.initialize_cache(metric_cache);
  parsed_metric
}

#[test]
fn overflow_drop() {
  let metric_cache = MetricCache::new(&Collector::default().scope("test"), Some(1));
  let state =
    SamplerState::<TestDurationJitter>::new(&metric_cache, &Collector::default().scope("test"));
  let now = Instant::now();
  let metric1 = make_metric("metric1", now, &metric_cache);
  let metric2 = make_metric("metric2", now, &metric_cache);

  assert_eq!(
    state.update_and_decide(now, &metric1, 1.seconds(), Duration::ZERO),
    SampleDecision::Forward
  );
  assert_eq!(
    state.update_and_decide(now, &metric2, 1.seconds(), Duration::ZERO),
    SampleDecision::Drop
  );
}

#[test]
fn sampler_state() {
  let metric_cache = MetricCache::new(&Collector::default().scope("test"), None);
  let state =
    SamplerState::<TestDurationJitter>::new(&metric_cache, &Collector::default().scope("test"));

  let now = Instant::now();
  let sample = make_metric("hello", now, &metric_cache);
  let emit_interval = 1.milliseconds();

  assert_eq!(
    state.update_and_decide(now, &sample, emit_interval, Duration::ZERO),
    SampleDecision::Forward
  );
  assert_eq!(
    state.update_and_decide(
      now + 1.std_nanoseconds(),
      &sample,
      emit_interval,
      Duration::ZERO
    ),
    SampleDecision::Drop
  );
  assert_eq!(
    state.update_and_decide(
      now + 2.std_nanoseconds(),
      &sample,
      emit_interval,
      Duration::ZERO
    ),
    SampleDecision::Drop
  );
  assert_eq!(
    state.update_and_decide(
      now + 3.std_nanoseconds(),
      &sample,
      emit_interval,
      Duration::ZERO
    ),
    SampleDecision::Drop
  );
  assert_eq!(
    state.update_and_decide(
      now + 4.std_nanoseconds(),
      &sample,
      emit_interval,
      Duration::ZERO
    ),
    SampleDecision::Drop
  );
  assert_eq!(
    state.update_and_decide(
      now + 1.std_milliseconds(),
      &sample,
      emit_interval,
      Duration::ZERO
    ),
    SampleDecision::Forward
  );
}

#[tokio::test(start_paused = true)]
async fn sampler_processor() {
  let config = SamplerConfig {
    emit_interval: 30.seconds().into_proto(),
    startup_interval: 5.seconds().into_proto(),
    ..Default::default()
  };
  let (mut helper, factory) = processor_factory_context_for_test();

  // Expect two calls
  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(2)
    .returning(|_| ());

  let sampler = Arc::new(
    SamplerProcessor::<TestDurationJitter>::new(config, factory)
      .await
      .unwrap(),
  );

  let now = Instant::now();
  let sample = make_metric("hello", now, &helper.metric_cache);

  // Drop - First call startup interval, 2nd still within interval.
  sampler.clone().recv_samples(vec![sample.clone()]).await;
  4.seconds().sleep().await;
  sampler.clone().recv_samples(vec![sample.clone()]).await;

  // Forward
  1.seconds().sleep().await;
  sampler.clone().recv_samples(vec![sample.clone()]).await;

  // Drop
  1.seconds().sleep().await;
  sampler.clone().recv_samples(vec![sample.clone()]).await;
  1.seconds().sleep().await;
  sampler.clone().recv_samples(vec![sample.clone()]).await;
  1.seconds().sleep().await;
  sampler.clone().recv_samples(vec![sample.clone()]).await;
  1.seconds().sleep().await;
  sampler.clone().recv_samples(vec![sample.clone()]).await;

  // Forward - Second call
  26.seconds().sleep().await;
  sampler.recv_samples(vec![sample.clone()]).await;
}
