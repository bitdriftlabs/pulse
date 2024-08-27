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
use async_trait::async_trait;
use prometheus::IntCounter;
use pulse_protobuf::protos::pulse::config::processor::v1::regex::RegexConfig;
use regex::bytes::RegexSet;
use std::sync::Arc;

//
// RegexProcessor
//

// A simple processor that performs regex set allow and deny. Broken out for efficiency reasons from
// the VRL abort handling.
pub struct RegexProcessor {
  dispatcher: Arc<dyn PipelineDispatch>,
  allow: RegexSet,
  deny: RegexSet,
  drop: IntCounter,
}

impl RegexProcessor {
  pub fn new(config: &RegexConfig, context: ProcessorFactoryContext) -> anyhow::Result<Self> {
    Ok(Self {
      dispatcher: context.dispatcher,
      allow: RegexSet::new(&config.allow)?,
      deny: RegexSet::new(&config.deny)?,
      drop: context.scope.counter("drop"),
    })
  }
}

#[async_trait]
impl PipelineProcessor for RegexProcessor {
  async fn recv_samples(self: Arc<Self>, samples: Vec<ParsedMetric>) {
    self
      .dispatcher
      .send(
        samples
          .into_iter()
          .filter_map(|metric| {
            if !self.allow.is_empty()
              && !self
                .allow
                .matches(metric.metric().get_id().name())
                .matched_any()
            {
              log::debug!(
                "dropping '{}' because not in allow list",
                metric.metric().get_id()
              );
              self.drop.inc();
              return None;
            }

            if !self.deny.is_empty()
              && self
                .deny
                .matches(metric.metric().get_id().name())
                .matched_any()
            {
              log::debug!(
                "dropping '{}' because in deny list",
                metric.metric().get_id()
              );
              self.drop.inc();
              return None;
            }

            Some(metric)
          })
          .collect(),
      )
      .await;
  }

  async fn start(self: Arc<Self>) {}
}
