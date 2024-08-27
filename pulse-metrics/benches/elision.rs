// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use bd_server_stats::stats::Collector;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use elision::elision_config::emit_config::Emit_type;
use elision::elision_config::{EmitConfig, ZeroElisionConfig};
use pulse_metrics::filters::filter::DynamicMetricFilter;
use pulse_metrics::filters::zero::ZeroFilter;
use pulse_metrics::metric_generator::MetricGenerator;
use pulse_metrics::pipeline::metric_cache::MetricCache;
use pulse_metrics::pipeline::processor::elision::state::{ElisionState, SkipDecider};
use pulse_metrics::pipeline::processor::sampler::SamplerState;
use pulse_metrics::pipeline::time::RealDurationJitter;
use pulse_metrics::test::make_carbon_wire_protocol;
use pulse_protobuf::protos::pulse::config::processor::v1::elision;
use std::sync::Arc;
use time::ext::NumericalDuration;
use time::Duration;
use tokio::time::Instant;

fn criterion_benchmark(c: &mut Criterion) {
  let mut generator = MetricGenerator::default();
  let metrics = generator.generate_metrics(100_000, &make_carbon_wire_protocol());
  c.bench_function("state insert", |b| {
    let state = ElisionState::new(
      MetricCache::new(&Collector::default().scope("test"), None),
      &Collector::default().scope("test"),
    );
    let skip_decider = SkipDecider::new(EmitConfig {
      emit_type: Emit_type::Ratio(0.1).into(),
      ..Default::default()
    });
    let metric_filters: &[DynamicMetricFilter] =
      &[Arc::new(ZeroFilter::new("", &ZeroElisionConfig::default()))];
    b.iter(|| {
      for m in &metrics {
        black_box(state.update_and_decide(m, metric_filters, &skip_decider, None));
      }
    });
  });

  c.bench_function("state update", |b| {
    let state = ElisionState::new(
      MetricCache::new(&Collector::default().scope("test"), None),
      &Collector::default().scope("test"),
    );
    let skip_decider = SkipDecider::new(EmitConfig {
      emit_type: Emit_type::Ratio(0.1).into(),
      ..Default::default()
    });
    let metric_filters: &[DynamicMetricFilter] =
      &[Arc::new(ZeroFilter::new("", &ZeroElisionConfig::default()))];
    for m in &metrics {
      black_box(state.update_and_decide(m, metric_filters, &skip_decider, None));
    }
    b.iter(|| {
      for m in &metrics {
        black_box(state.update_and_decide(m, metric_filters, &skip_decider, None));
      }
    });
  });

  c.bench_function("metric cache get", |b| {
    let metric_cache = MetricCache::new(&Collector::default().scope("test"), None);
    b.iter(|| {
      for m in &metrics {
        black_box(metric_cache.get(m.metric(), m.received_at()));
      }
    });
  });

  c.bench_function("sampler insert", |b| {
    let mut generator = MetricGenerator::default();
    let metrics = generator.generate_metrics(100_000, &make_carbon_wire_protocol());
    let metric_cache = MetricCache::new(&Collector::default().scope("test"), None);
    let state =
      SamplerState::<RealDurationJitter>::new(&metric_cache, &Collector::default().scope("test"));
    b.iter(|| {
      for m in &metrics {
        black_box(state.update_and_decide(Instant::now(), m, 1.seconds(), Duration::ZERO));
      }
    });
  });

  let mut group = c.benchmark_group("state update by number of replicas");
  for n in [1, 2, 3, 4] {
    group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
      let mut generator = MetricGenerator::default();
      let mut metrics = generator.generate_metrics(100_000, &make_carbon_wire_protocol());
      let metric_cache = MetricCache::new(&Collector::default().scope("test"), None);
      let states: Vec<ElisionState> = (0 .. n)
        .map(|_| ElisionState::new(metric_cache.clone(), &Collector::default().scope("test")))
        .collect();
      let skip_decider = SkipDecider::new(EmitConfig {
        emit_type: Emit_type::Ratio(0.1).into(),
        ..Default::default()
      });
      let metric_filters: &[DynamicMetricFilter] =
        &[Arc::new(ZeroFilter::new("", &ZeroElisionConfig::default()))];
      b.iter(|| {
        for m in &mut metrics {
          for state in &states {
            black_box(state.update_and_decide(m, metric_filters, &skip_decider, None));
          }
        }
      });
    });
  }
  group.finish();

  let mut group = c.benchmark_group("last elided lookup by size");
  // Create a new name generator to lookup. As we use the same seed, position
  // 0 will be the first metric name from the prior generator
  let mut lookup_generator = MetricGenerator::default();
  for size in [100, 1000, 100_000, 1_000_000] {
    let mut generator = MetricGenerator::default();
    let metrics = generator.generate_metrics(size, &make_carbon_wire_protocol());
    let metric_cache = MetricCache::new(&Collector::default().scope("test"), None);
    let state = ElisionState::new(metric_cache, &Collector::default().scope("test"));
    let skip_decider = SkipDecider::new(EmitConfig {
      emit_type: Emit_type::Ratio(0.1).into(),
      ..Default::default()
    });
    let metric_filters: &[DynamicMetricFilter] =
      &[Arc::new(ZeroFilter::new("", &ZeroElisionConfig::default()))];
    for m in &metrics {
      black_box(state.update_and_decide(m, metric_filters, &skip_decider, None));
    }

    group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, _| {
      let lookup_name = lookup_generator.default_name_terms();
      b.iter(|| {
        black_box(state.get_last_elided(&lookup_name));
      });
    });
  }
  group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
