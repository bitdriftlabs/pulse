// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./mod_test.rs"]
mod mod_test;

use super::{PipelineProcessor, ProcessorFactoryContext};
use crate::pipeline::PipelineDispatch;
use crate::protos::metric::{
  CounterType,
  Metric,
  MetricId,
  MetricType,
  MetricValue,
  ParsedMetric,
  TagValue,
};
use crate::vrl::ProgramWrapper;
use async_trait::async_trait;
use bd_log::warn_every;
use bd_server_stats::stats::Scope;
use bytes::BytesMut;
use itertools::Either;
use prometheus::IntCounter;
use pulse_protobuf::protos::pulse::config::processor::v1::mutate::MutateConfig;
use std::iter::{empty, once};
use std::sync::Arc;
use time::ext::NumericalDuration;
use vrl::compiler::ExpressionError;

//
// MutateStats
//

struct MutateStats {
  drop_abort: IntCounter,
  drop_error: IntCounter,
}

impl MutateStats {
  fn new(scope: &Scope) -> Self {
    Self {
      drop_abort: scope.counter("drop_abort"),
      drop_error: scope.counter("drop_error"),
    }
  }
}

//
// MutateProcessor
//

/// A job that mutates metric names/fields based on associated metadata.
pub struct MutateProcessor {
  program: ProgramWrapper,
  dispatcher: Arc<dyn PipelineDispatch>,
  stats: MutateStats,
}

impl MutateProcessor {
  pub fn new(config: &MutateConfig, context: ProcessorFactoryContext) -> anyhow::Result<Self> {
    Ok(Self {
      program: ProgramWrapper::new(&config.vrl_program)?,
      dispatcher: context.dispatcher,
      stats: MutateStats::new(&context.scope),
    })
  }

  fn maybe_flatten_prom_histogram_and_summary(
    sample: ParsedMetric,
  ) -> impl Iterator<Item = ParsedMetric> {
    fn make_metric(
      sample: &ParsedMetric,
      name_suffix: &str,
      new_tags: Vec<TagValue>,
      metric_type: MetricType,
      value: f64,
    ) -> Option<ParsedMetric> {
      let name = if name_suffix.is_empty() {
        sample.metric().get_id().name().clone()
      } else {
        let mut name =
          BytesMut::with_capacity(sample.metric().get_id().name().len() + name_suffix.len());
        name.extend_from_slice(sample.metric().get_id().name());
        name.extend_from_slice(name_suffix.as_bytes());
        name.freeze()
      };

      let metric = Metric::new(
        MetricId::new(name, Some(metric_type), new_tags, false).ok()?,
        sample.metric().sample_rate,
        sample.metric().timestamp,
        MetricValue::Simple(value),
      );

      Some(ParsedMetric::new(
        metric,
        sample.source().clone(),
        sample.received_at(),
        sample.downstream_id().clone(),
      ))
    }

    match sample.metric().get_id().mtype() {
      Some(MetricType::Histogram) => {
        let histogram = sample.metric().value.to_histogram().clone();
        let sum_metric = {
          make_metric(
            &sample,
            "_sum",
            sample.metric().get_id().tags().to_vec(),
            MetricType::Counter(CounterType::Absolute),
            histogram.sample_sum,
          )
        };
        let count_metric = {
          make_metric(
            &sample,
            "_count",
            sample.metric().get_id().tags().to_vec(),
            MetricType::Counter(CounterType::Absolute),
            histogram.sample_count,
          )
        };
        let inf_metric = {
          let mut tags = sample.metric().get_id().tags().to_vec();
          tags.push(TagValue {
            tag: "le".into(),
            value: "+Inf".into(),
          });
          make_metric(
            &sample,
            "_bucket",
            tags,
            MetricType::Counter(CounterType::Absolute),
            histogram.sample_count,
          )
        };
        Either::Left(
          histogram
            .buckets
            .into_iter()
            .filter_map(move |b| {
              let mut tags = sample.metric().get_id().tags().to_vec();
              tags.push(TagValue {
                tag: "le".into(),
                value: b.le.to_string().into(),
              });
              make_metric(
                &sample,
                "_bucket",
                tags,
                MetricType::Counter(CounterType::Absolute),
                b.count,
              )
            })
            .chain(inf_metric)
            .chain(sum_metric)
            .chain(count_metric),
        )
      },
      Some(MetricType::Summary) => {
        let summary = sample.metric().value.to_summary().clone();
        let sum_metric = {
          make_metric(
            &sample,
            "_sum",
            sample.metric().get_id().tags().to_vec(),
            MetricType::Counter(CounterType::Absolute),
            summary.sample_sum,
          )
        };
        let count_metric = {
          make_metric(
            &sample,
            "_count",
            sample.metric().get_id().tags().to_vec(),
            MetricType::Counter(CounterType::Absolute),
            summary.sample_count,
          )
        };
        Either::Right(Either::Left(
          summary
            .quantiles
            .into_iter()
            .filter_map(move |q| {
              let mut tags = sample.metric().get_id().tags().to_vec();
              tags.push(TagValue {
                tag: "quantile".into(),
                value: q.quantile.to_string().into(),
              });
              make_metric(&sample, "", tags, MetricType::Gauge, q.value)
            })
            .chain(sum_metric)
            .chain(count_metric),
        ))
      },
      _ => Either::Right(Either::Right(once(sample))),
    }
  }
}

#[async_trait]
impl PipelineProcessor for MutateProcessor {
  async fn recv_samples(self: Arc<Self>, samples: Vec<ParsedMetric>) {
    let samples: Vec<_> = samples
      .into_iter()
      .flat_map(|mut sample| {
        let result = self.program.run_with_metric(&mut sample);
        match result.resolved {
          Ok(_) | Err(ExpressionError::Return { .. }) => {
            if result.flatten_prom_histogram_and_summary {
              Either::Left(Self::maybe_flatten_prom_histogram_and_summary(sample))
            } else {
              Either::Right(Either::Left(once(sample)))
            }
          },
          Err(ExpressionError::Abort { .. }) => {
            // We assume that explicit aborts are intentional.
            self.stats.drop_abort.inc();
            Either::Right(Either::Right(empty()))
          },
          Err(e) => {
            // We assume that errors are not intentional and are either an issue in the environment
            // or a bug in the script so in this case warn the user.
            warn_every!(1.minutes(), "metric drop due to VRL error: {}", e);
            self.stats.drop_error.inc();
            Either::Right(Either::Right(empty()))
          },
        }
      })
      .collect();

    if !samples.is_empty() {
      self.dispatcher.send(samples).await;
    }
  }

  async fn start(self: Arc<Self>) {}
}
