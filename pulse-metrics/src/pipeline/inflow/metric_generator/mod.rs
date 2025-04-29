// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::PipelineInflow;
use crate::metric_generator::MetricGenerator;
use crate::pipeline::{InflowFactoryContext, PipelineDispatch};
use crate::protos::metric::ParsedMetric;
use async_trait::async_trait;
use bd_shutdown::{ComponentShutdown, ComponentShutdownTriggerHandle};
use bd_time::TimeDurationExt;
use pulse_common::proto::ProtoDurationToStdDuration;
use pulse_common::{LossyFloatToInt, LossyIntToFloat};
use pulse_protobuf::protos::pulse::config::inflow::v1::metric_generator::MetricGeneratorConfig;
use std::sync::Arc;
use time::Duration;

const fn default_n_tasks() -> u64 {
  16
}

const fn default_flush_interval() -> Duration {
  Duration::seconds(1)
}

pub(super) struct MetricGeneratorInflow {
  config: MetricGeneratorConfig,
  dispatcher: Arc<dyn PipelineDispatch>,
  shutdown_trigger_handle: ComponentShutdownTriggerHandle,
}

impl MetricGeneratorInflow {
  pub fn new(config: MetricGeneratorConfig, context: InflowFactoryContext) -> Self {
    Self {
      config,
      dispatcher: context.dispatcher,
      shutdown_trigger_handle: context.shutdown_trigger_handle,
    }
  }
}

#[async_trait]
impl PipelineInflow for MetricGeneratorInflow {
  async fn start(self: Arc<Self>) {
    let mut generator = MetricGenerator::default();
    let lines_per_tasks = (self.config.batch_size.unwrap_or(128).lossy_to_f64()
      / self
        .config
        .n_tasks
        .unwrap_or(default_n_tasks())
        .lossy_to_f64())
    .ceil()
    .lossy_to_usize();
    let protocol = self.config.protocol.clone().unwrap();
    for _ in 0 .. self.config.n_tasks.unwrap_or(default_n_tasks()) {
      let samples = generator.generate_metrics(lines_per_tasks, &protocol);
      tokio::spawn(dispatch_samples_loop(
        samples,
        self.config.clone(),
        self.dispatcher.clone(),
        self.shutdown_trigger_handle.make_shutdown(),
      ));
    }
  }
}

async fn dispatch_samples_loop(
  samples: Vec<ParsedMetric>,
  config: MetricGeneratorConfig,
  dispatcher: Arc<dyn PipelineDispatch>,
  mut shutdown: ComponentShutdown,
) {
  let shutdown = shutdown.cancelled();
  tokio::pin!(shutdown);

  loop {
    dispatcher.send(samples.clone()).await;
    tokio::select! {
      () = &mut shutdown => {
        break;
      }
      () = config.flush_interval.unwrap_duration_or(default_flush_interval()).sleep() => {}
    }
  }
}
