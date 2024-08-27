// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::PipelineProcessor;
use crate::pipeline::metric_cache::MetricCache;
use crate::pipeline::{PipelineDispatch, ProcessorFactoryContext};
use crate::protos::metric::ParsedMetric;
use async_trait::async_trait;
use std::sync::Arc;

//
// PopulateCacheProcessor
//

// A pipeline processor that primes the cache for each metric before potentially forking out to
// multiple concurrent processors.
pub struct PopulateCacheProcessor {
  metric_cache: Arc<MetricCache>,
  dispatcher: Arc<dyn PipelineDispatch>,
}

impl PopulateCacheProcessor {
  pub fn new(context: ProcessorFactoryContext) -> Self {
    Self {
      metric_cache: context.metric_cache,
      dispatcher: context.dispatcher,
    }
  }
}

#[async_trait]
impl PipelineProcessor for PopulateCacheProcessor {
  async fn recv_samples(self: Arc<Self>, mut samples: Vec<ParsedMetric>) {
    for sample in &mut samples {
      sample.initialize_cache(&self.metric_cache);
    }

    self.dispatcher.send(samples).await;
  }

  async fn start(self: Arc<Self>) {}
}
