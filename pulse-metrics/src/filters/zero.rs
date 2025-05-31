// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./zero_test.rs"]
mod zero_test;

use super::filter::{MetricFilter, MetricFilterDecision};
use crate::protos::metric::{CounterType, MetricType, MetricValue, ParsedMetric};
use elision_config::ZeroElisionConfig;
use pulse_protobuf::protos::pulse::config::processor::v1::elision::elision_config;

pub struct ZeroFilter {
  name: String,
  config: ZeroElisionConfig,
}

impl ZeroFilter {
  #[must_use]
  pub fn new(name: &str, config: &ZeroElisionConfig) -> Self {
    Self {
      name: name.to_string(),
      config: config.clone(),
    }
  }
}

impl MetricFilter for ZeroFilter {
  fn decide(
    &self,
    metric: &ParsedMetric,
    last_value: &Option<MetricValue>,
    last_value_type: Option<MetricType>,
  ) -> super::filter::MetricFilterDecision {
    let Some(last_value) = last_value else {
      return MetricFilterDecision::Initializing;
    };

    match (metric.metric().get_id().mtype(), last_value_type) {
      (
        Some(MetricType::Counter(CounterType::Absolute)),
        Some(MetricType::Counter(CounterType::Absolute)),
      ) => {
        if self.config.counters.absolute_counters.elide_if_no_change {
          return if (metric.metric().value.to_simple() - last_value.to_simple()).abs()
            < f64::EPSILON
          {
            MetricFilterDecision::Fail
          } else {
            MetricFilterDecision::NotCovered
          };
        }
      },
      (Some(MetricType::Histogram), Some(MetricType::Histogram)) => {
        // Generally histograms are absolute counters, though they may have been made into delta
        // counters via the aggregation processor.
        if self.config.histograms.elide_if_no_change {
          return if (metric.metric().value.to_histogram().sample_count
            - last_value.to_histogram().sample_count)
            .abs()
            < f64::EPSILON
          {
            MetricFilterDecision::Fail
          } else {
            MetricFilterDecision::NotCovered
          };
        }
        return if metric.metric().value.to_histogram().sample_count < f64::EPSILON
          && last_value.to_histogram().sample_count < f64::EPSILON
        {
          MetricFilterDecision::Fail
        } else {
          MetricFilterDecision::NotCovered
        };
      },
      (Some(MetricType::Summary), Some(MetricType::Summary)) => {
        // We assume summary counts are absolute counters.
        return if (metric.metric().value.to_summary().sample_count
          - last_value.to_summary().sample_count)
          .abs()
          < f64::EPSILON
        {
          MetricFilterDecision::Fail
        } else {
          MetricFilterDecision::NotCovered
        };
      },
      (..) => {},
    }

    match (&metric.metric().value, last_value) {
      (MetricValue::Simple(value), MetricValue::Simple(last_value)) => {
        if *value < f64::EPSILON && *last_value < f64::EPSILON {
          MetricFilterDecision::Fail
        } else {
          MetricFilterDecision::NotCovered
        }
      },
      (..) => MetricFilterDecision::NotCovered,
    }
  }

  fn name(&self) -> &str {
    &self.name
  }

  fn match_decision(&self) -> MetricFilterDecision {
    MetricFilterDecision::Fail
  }
}
