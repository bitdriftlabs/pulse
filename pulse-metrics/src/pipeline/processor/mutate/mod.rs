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
use crate::protos::metric::ParsedMetric;
use crate::vrl::ProgramWrapper;
use async_trait::async_trait;
use bd_log::warn_every;
use bd_server_stats::stats::Scope;
use prometheus::IntCounter;
use pulse_protobuf::protos::pulse::config::processor::v1::mutate::MutateConfig;
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
}

#[async_trait]
impl PipelineProcessor for MutateProcessor {
  async fn recv_samples(self: Arc<Self>, samples: Vec<ParsedMetric>) {
    let samples: Vec<_> = samples
      .into_iter()
      .filter_map(|mut sample| {
        match self.program.run_with_metric(&mut sample) {
          Ok(_) | Err(ExpressionError::Return { .. }) => Some(sample),
          Err(ExpressionError::Abort { .. }) => {
            // We assume that explicit aborts are intentional.
            self.stats.drop_abort.inc();
            None
          },
          Err(e) => {
            // We assume that errors are not intentional and are either an issue in the environment
            // or a bug in the script so in this case warn the user.
            warn_every!(1.minutes(), "metric drop due to VRL error: {}", e);
            self.stats.drop_error.inc();
            None
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
