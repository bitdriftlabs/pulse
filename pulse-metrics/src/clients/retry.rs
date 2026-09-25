// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/licenses/strict/1.0.0.txt

#[cfg(test)]
#[path = "./retry_test.rs"]
mod retry_test;

pub use bd_otlp_metrics::Retry;
use bd_otlp_metrics::RetryConfig;
use pulse_protobuf::protos::pulse::config::common::v1::retry::RetryPolicy;

pub fn make_retry(config: &RetryPolicy) -> anyhow::Result<std::sync::Arc<Retry>> {
  Retry::new(RetryConfig {
    budget: config.budget,
    max_retries: config.max_retries,
  })
}
