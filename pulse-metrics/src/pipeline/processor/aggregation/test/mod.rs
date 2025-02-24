// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::AggregationProcessor;
use crate::pipeline::processor::PipelineProcessor;
use crate::pipeline::time::TestDurationJitter;
use crate::protos::metric::{
  CounterType,
  DownstreamId,
  MetricSource,
  MetricType,
  MetricValue,
  ParsedMetric,
};
use crate::test::thread_synchronizer::ThreadSynchronizer;
use crate::test::{
  ProcessorFactoryContextHelper,
  clean_timestamps,
  make_metric_ex,
  processor_factory_context_for_test,
};
use aggregation_config::counters::{AbsoluteCounters, Extended as ExtendedCounters};
use aggregation_config::gauges::Extended as ExtendedGauges;
use aggregation_config::quantile_timers::Extended as ExtendedQuantileTimers;
use aggregation_config::{Counters, Gauges};
use bd_test_helpers::make_mut;
use bd_time::ToProtoDuration;
use pretty_assertions::assert_eq;
use pulse_protobuf::protos::pulse::config::processor::v1::aggregation::aggregation_config::{
  QuantileTimers,
  ReservoirTimers,
  Timer_type,
};
use pulse_protobuf::protos::pulse::config::processor::v1::aggregation::{
  AggregationConfig,
  aggregation_config,
};
use std::sync::Arc;
use time::Duration;
use time::ext::NumericalDuration;

#[derive(Copy, Clone)]
pub enum TimerType {
  QuantileExtended,
  Reservoir,
}

//
// HelperBuilder
//

#[allow(clippy::struct_excessive_bools)]
#[derive(Default)]
pub struct HelperBuilder {
  extended_counters: bool,
  extended_gauges: bool,
  timer_type: Option<TimerType>,
  enable_last_aggregation_admin_endpoint: bool,
  emit_absolute_counters_as_delta_rate: bool,
  flush_interval: Option<Duration>,
  flush_jitter: Option<Duration>,
  emit_prometheus_stale_markers: bool,
}

impl HelperBuilder {
  pub const fn extended_counters(mut self) -> Self {
    self.extended_counters = true;
    self
  }

  pub const fn extended_gauges(mut self) -> Self {
    self.extended_gauges = true;
    self
  }

  pub const fn timer_type(mut self, timer_type: TimerType) -> Self {
    self.timer_type = Some(timer_type);
    self
  }

  pub const fn enable_last_aggregation_admin_endpoint(mut self) -> Self {
    self.enable_last_aggregation_admin_endpoint = true;
    self
  }

  pub const fn emit_absolute_counters_as_delta_rate(mut self) -> Self {
    self.emit_absolute_counters_as_delta_rate = true;
    self
  }

  pub const fn flush_interval(mut self, flush_interval: Duration) -> Self {
    self.flush_interval = Some(flush_interval);
    self
  }

  pub const fn flush_jitter(mut self, flush_jitter: Duration) -> Self {
    self.flush_jitter = Some(flush_jitter);
    self
  }

  pub const fn enable_emit_prometheus_stale_markers(mut self) -> Self {
    self.emit_prometheus_stale_markers = true;
    self
  }

  pub async fn build(self) -> Helper {
    Helper::new(
      self.extended_counters,
      self.extended_gauges,
      self.timer_type,
      self.enable_last_aggregation_admin_endpoint,
      self.emit_absolute_counters_as_delta_rate,
      self.flush_interval,
      self.flush_jitter,
      self.emit_prometheus_stale_markers,
    )
    .await
  }
}

//
// Helper
//

pub struct Helper {
  processor: Arc<AggregationProcessor>,
  pub helper: ProcessorFactoryContextHelper,
}

impl Helper {
  #[allow(clippy::fn_params_excessive_bools)]
  async fn new(
    extended_counters: bool,
    extended_gauges: bool,
    timer_type: Option<TimerType>,
    enable_last_aggregation_admin_endpoint: bool,
    emit_absolute_counters_as_delta_rate: bool,
    flush_interval: Option<Duration>,
    flush_jitter: Option<Duration>,
    emit_prometheus_stale_markers: bool,
  ) -> Self {
    let config = AggregationConfig {
      timer_type: timer_type.map(|timer_type| match timer_type {
        TimerType::QuantileExtended => Timer_type::QuantileTimers(QuantileTimers {
          prefix: Some("timers.".into()),
          eps: Some(0.001),
          quantiles: vec![0.5, 0.95, 0.99, 0.999],
          extended: matches!(timer_type, TimerType::QuantileExtended)
            .then_some(ExtendedQuantileTimers {
              mean: true,
              lower: true,
              upper: true,
              count: true,
              rate: true,
              ..Default::default()
            })
            .into(),
          ..Default::default()
        }),
        TimerType::Reservoir => Timer_type::ReservoirTimers(ReservoirTimers::default()),
      }),
      counters: Some(Counters {
        prefix: Some("counters.".into()),
        extended: extended_counters
          .then_some(ExtendedCounters {
            count: true,
            sum: true,
            rate: true,
            lower: true,
            upper: true,
            ..Default::default()
          })
          .into(),
        absolute_counters: Some(AbsoluteCounters {
          emit_as_delta_rate: emit_absolute_counters_as_delta_rate,
          ..Default::default()
        })
        .into(),
        ..Default::default()
      })
      .into(),
      gauges: Some(Gauges {
        prefix: Some("gauges.".into()),
        extended: extended_gauges
          .then_some(ExtendedGauges {
            sum: true,
            mean: true,
            min: true,
            max: true,
            ..Default::default()
          })
          .into(),
        ..Default::default()
      })
      .into(),
      flush_interval: flush_interval.unwrap_or_else(|| 1.minutes()).into_proto(),
      enable_last_aggregation_admin_endpoint,
      post_flush_send_jitter: flush_jitter
        .map(bd_time::ToProtoDuration::into_proto)
        .unwrap_or_default(),
      emit_prometheus_stale_markers,
      ..Default::default()
    };
    let (helper, factory) = processor_factory_context_for_test();
    let processor = AggregationProcessor::new::<TestDurationJitter>(config, factory)
      .await
      .unwrap();
    Self { processor, helper }
  }

  pub fn make_metric_ex(
    &self,
    name: &str,
    metric_type: Option<MetricType>,
    sample_rate: Option<f64>,
    value: MetricValue,
    metric_source: MetricSource,
    downstream_id: DownstreamId,
  ) -> ParsedMetric {
    let mut parsed_metric = make_metric_ex(
      name,
      &[],
      0,
      metric_type,
      sample_rate,
      value,
      metric_source,
      downstream_id,
      None,
    );
    parsed_metric.initialize_cache(&self.helper.metric_cache);
    parsed_metric
  }

  pub fn make_abs_counter(
    &self,
    name: &'static str,
    value: f64,
    ip_addr: &'static str,
  ) -> ParsedMetric {
    self.make_metric_ex(
      name,
      Some(MetricType::Counter(CounterType::Absolute)),
      None,
      MetricValue::Simple(value),
      MetricSource::PromRemoteWrite,
      DownstreamId::IpAddress(ip_addr.parse().unwrap()),
    )
  }

  pub fn make_metric(
    &self,
    name: &'static str,
    metric_type: MetricType,
    value: MetricValue,
  ) -> ParsedMetric {
    self.make_metric_ex(
      name,
      Some(metric_type),
      None,
      value,
      MetricSource::PromRemoteWrite,
      DownstreamId::LocalOrigin,
    )
  }

  pub fn expected_metric(
    &self,
    name: &'static str,
    value: MetricValue,
    metric_type: MetricType,
  ) -> ParsedMetric {
    self.make_metric_ex(
      name,
      Some(metric_type),
      None,
      value,
      MetricSource::Aggregation { prom_source: true },
      DownstreamId::LocalOrigin,
    )
  }

  pub fn expect_gauge(&self, name: &'static str, value: f64) -> ParsedMetric {
    self.expected_metric(name, MetricValue::Simple(value), MetricType::Gauge)
  }

  pub fn expect_dcounter(&self, name: &'static str, value: f64) -> ParsedMetric {
    self.expected_metric(
      name,
      MetricValue::Simple(value),
      MetricType::Counter(CounterType::Delta),
    )
  }

  pub fn expect_acounter(&self, name: &'static str, value: f64) -> ParsedMetric {
    self.expected_metric(
      name,
      MetricValue::Simple(value),
      MetricType::Counter(CounterType::Absolute),
    )
  }

  pub fn expect_dispatch(&mut self, expected: Vec<ParsedMetric>) {
    make_mut(&mut self.helper.dispatcher)
      .expect_send()
      .times(1)
      .return_once(move |samples| {
        assert_eq!(clean_timestamps(samples), expected);
      });
  }

  pub async fn recv(&self, metrics: Vec<ParsedMetric>) {
    self.processor.clone().recv_samples(metrics).await;
  }

  pub fn thread_synchronizer(&self) -> &ThreadSynchronizer {
    &self.processor.thread_synchronizer
  }
}
