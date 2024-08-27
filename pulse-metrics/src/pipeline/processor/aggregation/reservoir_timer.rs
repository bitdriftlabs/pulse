// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::make_metric;
use crate::protos::metric::{MetricId, MetricType, MetricValue, ParsedMetric};
use rand::{RngCore, SeedableRng};
use rand_xoshiro::Xoshiro128StarStar;
use std::cell::RefCell;
use tokio::time::Instant;

//
// ReservoirTimerAggregation
//

// This is a basic reservoir sampling implementation adapted from statsrelay. A reservoir is kept
// with a maximum number of samples during each interval. Extra samples are randomly replaced with
// a chance that decreases as the number of overall samples increases.
pub(super) struct ReservoirTimerAggregation {
  reservoir: Vec<f64>,
  filled_count: u32,
  reservoir_size: u32,
  count: f64,
}

impl ReservoirTimerAggregation {
  pub(super) fn new(reservoir_size: u32) -> Self {
    Self {
      reservoir: Vec::with_capacity(reservoir_size as usize),
      filled_count: 0,
      reservoir_size,
      count: 0.0,
    }
  }

  pub(super) fn aggregate(&mut self, value: f64, sample_rate: f64) {
    thread_local! {
      // Fast non crypto rng.
      static RANDOM: RefCell<Xoshiro128StarStar> = RefCell::new(Xoshiro128StarStar::from_entropy());
    }

    // Do an initial fill if we haven't filled the full reservoir.
    if self.reservoir.len() < self.reservoir_size as usize {
      self.reservoir.push(value);
    } else {
      match RANDOM.with(|r| r.borrow_mut().next_u32()) % self.filled_count {
        idx if idx < self.reservoir_size => self.reservoir[idx as usize] = value,
        _ => (),
      }
    }
    // Keep track of a sample rate scaled count independently from the
    // reservoir sample fill
    self.count += 1.0 / sample_rate;
    self.filled_count += 1;
  }

  pub(super) fn produce_metrics(
    &mut self,
    metric_id: &MetricId,
    timestamp: u64,
    now: Instant,
    prom_source: bool,
  ) -> Vec<Option<ParsedMetric>> {
    // Derive the sample rate based on the number of overall samples we got.
    let sample_rate = self.reservoir.len() as f64 / self.count;
    self.count = 0.0;
    self.filled_count = 0;
    self
      .reservoir
      .drain(..)
      .map(|t| {
        make_metric(
          metric_id.name().clone(),
          metric_id.tags().to_vec(),
          MetricValue::Simple(t),
          Some(sample_rate),
          timestamp,
          now,
          MetricType::Timer,
          prom_source,
        )
      })
      .collect()
  }
}
