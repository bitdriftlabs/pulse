// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::protos::metric::{Metric, MetricType, MetricValue};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MetricFilterDecision {
  Initializing,
  Pass,
  Fail,
  NotCovered,
}

pub trait MetricFilter {
  fn decide(
    &self,
    metric: &Metric,
    last_value: &Option<MetricValue>,
    last_value_type: Option<MetricType>,
  ) -> MetricFilterDecision;
  fn name(&self) -> &str;
  fn match_decision(&self) -> MetricFilterDecision;
}

pub type DynamicMetricFilter = std::sync::Arc<dyn MetricFilter + Send + Sync>;

pub struct MockMetricFilter {
  pub match_decision: MetricFilterDecision,
  pub name: String,
}

impl MockMetricFilter {
  #[must_use]
  pub fn new(match_decision: MetricFilterDecision, name: &str) -> Self {
    Self {
      match_decision,
      name: name.to_string(),
    }
  }
}

impl MetricFilter for MockMetricFilter {
  fn decide(
    &self,
    _: &Metric,
    _: &Option<MetricValue>,
    _: Option<MetricType>,
  ) -> MetricFilterDecision {
    self.match_decision
  }

  fn name(&self) -> &str {
    &self.name
  }

  fn match_decision(&self) -> MetricFilterDecision {
    self.match_decision
  }
}
