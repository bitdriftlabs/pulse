// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::protos::metric::{
  DownstreamId,
  Metric,
  MetricId,
  MetricSource,
  MetricValue,
  ParsedMetric,
  TagValue,
};
use pulse_protobuf::protos::pulse::config::common::v1::common::wire_protocol::Protocol_type;
use pulse_protobuf::protos::pulse::config::common::v1::common::WireProtocol;
use rand::seq::SliceRandom;
use rand::Rng;
use rand_xoshiro::rand_core::SeedableRng;
use rand_xoshiro::Xoroshiro128StarStar;
use std::time::Instant;

const WORDS: &[&str] = &[
  "apple",
  "pear",
  "memory",
  "disk",
  "sum",
  "total",
  "peanuts",
  "usb",
  "errors",
  "help",
  "panic",
  "envoy",
  "cheese",
  "pizza",
  "vanilla",
  "watermelon",
  "egg",
  "dill",
  "nut",
  "mint",
  "jigsaw",
  "ocean",
  "panda",
  "basil",
  "olive",
  "quince",
  "waterfall",
  "universe",
  "cactus",
  "denim",
  "umami",
  "vinegar",
  "kale",
  "lettuce",
  "mint",
  "fennel",
  "ginger",
  "honeydew",
  "quilt",
  "rainbow",
  "years",
  "yeast",
  "wasabi",
  "amethyst",
  "diamond",
  "ruby",
  "jalapeno",
  "users",
  "rides",
  "total_things",
  "long_words",
  "runtime",
  "rss",
  "peak",
  "disaster",
  "eat",
  "drive_into_it",
];

pub struct MetricGenerator {
  rng: Xoroshiro128StarStar,
  sep: &'static str,
}

impl Default for MetricGenerator {
  fn default() -> Self {
    let rng = Xoroshiro128StarStar::seed_from_u64(0);
    Self { rng, sep: "." }
  }
}

impl MetricGenerator {
  #[must_use]
  pub fn new(sep: &'static str) -> Self {
    let rng = Xoroshiro128StarStar::seed_from_u64(0);
    Self { rng, sep }
  }

  pub fn name_terms(&mut self, terms: usize) -> String {
    let terms: Vec<_> = WORDS
      .choose_multiple(&mut self.rng, terms)
      .copied()
      .collect();
    terms.join(self.sep)
  }

  pub fn default_name_terms(&mut self) -> String {
    let range = self.rng.gen_range(5 .. 10);
    self.name_terms(range)
  }

  pub fn generate_metrics(
    &mut self,
    count: usize,
    wire_protocol: &WireProtocol,
  ) -> Vec<ParsedMetric> {
    (0 .. count)
      .map(|i| {
        let name_terms = self.rng.gen_range(5 .. 10);
        let name = self.name_terms(name_terms);

        let num_tags: usize = self.rng.gen_range(1 .. 10);

        let mut tags: Vec<_> = (0 .. num_tags).map(|_| self.name_terms(1)).collect();
        tags.sort();
        tags.dedup();
        let metric = Metric::new(
          MetricId::new(
            name.into(),
            None,
            tags
              .into_iter()
              .map(|k| TagValue {
                tag: k.into(),
                value: self.name_terms(1).into(),
              })
              .collect(),
            false,
          )
          .unwrap(),
          None,
          i as u64,
          MetricValue::Simple(0.0),
        );

        let bytes = metric.to_wire_format(wire_protocol);
        let source = match wire_protocol.protocol_type {
          Some(Protocol_type::Statsd(_)) => MetricSource::Statsd(bytes),
          Some(Protocol_type::Carbon(_)) => MetricSource::Carbon(bytes),
          None => unreachable!("pgv"),
        };
        ParsedMetric::new(metric, source, Instant::now(), DownstreamId::LocalOrigin)
      })
      .collect()
  }
}
