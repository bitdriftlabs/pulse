// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::*;
use std::collections::HashMap;

#[test]
pub fn test_carbon_output() {
  let provider = StatsProvider::new(None, vec![]);
  let scope = provider.collector().scope("prefix");
  let ctr1 = scope.counter("counter");
  ctr1.inc();
  let ctr2 = scope.counter("another_counter");
  ctr2.inc();
  ctr2.inc();

  let output = provider.carbon_output(123_456);
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "prefix.another_counter.count 2 123456 source=\"0\"\nprefix.counter.count 1 123456 \
     source=\"0\"\nstats.label_tracker.label_overflow.count 0 123456 source=\"0\"\n"
  );
}

#[test]
pub fn test_carbon_output_labels() {
  let provider = StatsProvider::new(None, vec![]);
  let scope = provider.collector().scope("prefix");
  let ctr1 = scope.counter("counter");
  ctr1.inc();
  let mut labels = HashMap::new();
  labels.insert(String::from("region"), String::from("IAD"));
  labels.insert(String::from("hello"), String::from("world"));
  let ctr2 = scope.counter_with_labels("another_counter", labels);
  ctr2.inc();
  ctr2.inc();

  let output = provider.carbon_output(123_456);
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "prefix.another_counter.count 2 123456 source=\"0\" hello=\"world\" \
     region=\"IAD\"\nprefix.counter.count 1 123456 \
     source=\"0\"\nstats.label_tracker.label_overflow.count 0 123456 source=\"0\"\n"
  );
}

#[test]
pub fn test_carbon_output_scope_labels() {
  let provider = StatsProvider::new(None, vec![]);
  let scope = provider.collector().scope("prefix");
  let ctr1 = scope.counter("counter");
  ctr1.inc();
  let mut labels = HashMap::new();
  labels.insert(String::from("region"), String::from("IAD"));
  labels.insert(String::from("hello"), String::from("world"));
  let ctr2 = scope
    .scope_with_labels("", labels)
    .counter("another_counter");
  ctr2.inc();
  ctr2.inc();

  let output = provider.carbon_output(123_456);
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "prefix.another_counter.count 2 123456 source=\"0\" hello=\"world\" \
     region=\"IAD\"\nprefix.counter.count 1 123456 \
     source=\"0\"\nstats.label_tracker.label_overflow.count 0 123456 source=\"0\"\n"
  );
}

#[test]
pub fn test_carbon_output_labels_empty_name() {
  let provider = StatsProvider::new(None, vec![]);
  let scope = provider.collector().scope("prefix");
  let ctr1 = scope.counter("counter");
  ctr1.inc();
  let mut labels = HashMap::new();
  labels.insert(String::from("region"), String::from("IAD"));
  labels.insert(String::from("hello"), String::from("world"));
  let ctr2 = scope.counter_with_labels("", labels);
  ctr2.inc();
  ctr2.inc();

  let output = provider.carbon_output(123_456);
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "prefix..count 2 123456 source=\"0\" hello=\"world\" region=\"IAD\"\nprefix.counter.count 1 \
     123456 source=\"0\"\nstats.label_tracker.label_overflow.count 0 123456 source=\"0\"\n"
  );
}

#[test]
pub fn test_carbon_output_gauge() {
  let provider = StatsProvider::new(None, vec![]);
  let scope = provider.collector().scope("prefix");
  let c = scope.counter("counter");
  c.inc();
  let g = scope.gauge("some_gauge");
  g.set(2);

  let output = provider.carbon_output(123_456);
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "prefix.counter.count 1 123456 source=\"0\"\nprefix.some_gauge.gauge 2 123456 \
     source=\"0\"\nstats.label_tracker.label_overflow.count 0 123456 source=\"0\"\n"
  );
}

#[test]
pub fn test_carbon_output_histogram() {
  let provider = StatsProvider::new(None, vec![]);
  let scope = provider.collector().scope("prefix");
  let h1 = scope.histogram("histogram");
  h1.observe(0.1);

  let output = provider.carbon_output(123_456);
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "prefix.histogram.count 1 123456 source=\"0\"\nprefix.histogram.sum 0.1 123456 \
     source=\"0\"\nprefix.histogram.bucket 0 123456 source=\"0\" \
     le=\"0.005\"\nprefix.histogram.bucket 0 123456 source=\"0\" \
     le=\"0.01\"\nprefix.histogram.bucket 0 123456 source=\"0\" \
     le=\"0.025\"\nprefix.histogram.bucket 0 123456 source=\"0\" \
     le=\"0.05\"\nprefix.histogram.bucket 1 123456 source=\"0\" \
     le=\"0.1\"\nprefix.histogram.bucket 1 123456 source=\"0\" \
     le=\"0.25\"\nprefix.histogram.bucket 1 123456 source=\"0\" \
     le=\"0.5\"\nprefix.histogram.bucket 1 123456 source=\"0\" le=\"1\"\nprefix.histogram.bucket \
     1 123456 source=\"0\" le=\"2.5\"\nprefix.histogram.bucket 1 123456 source=\"0\" \
     le=\"5\"\nprefix.histogram.bucket 1 123456 source=\"0\" le=\"10\"\nprefix.histogram.bucket 1 \
     123456 source=\"0\" le=\"+Inf\"\nstats.label_tracker.label_overflow.count 0 123456 \
     source=\"0\"\n"
  );
}

#[test]
pub fn test_carbon_output_histogram_custom_buckets() {
  let provider = StatsProvider::new(None, vec![]);
  let scope = provider.collector().scope("prefix");
  let h1 = scope.histogram_with_buckets("histogram", &[0.01, 0.05]);
  h1.observe(0.1);

  let output = provider.carbon_output(123_456);
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "prefix.histogram.count 1 123456 source=\"0\"\nprefix.histogram.sum 0.1 123456 \
     source=\"0\"\nprefix.histogram.bucket 0 123456 source=\"0\" \
     le=\"0.01\"\nprefix.histogram.bucket 0 123456 source=\"0\" \
     le=\"0.05\"\nprefix.histogram.bucket 1 123456 source=\"0\" \
     le=\"+Inf\"\nstats.label_tracker.label_overflow.count 0 123456 source=\"0\"\n"
  );
}

#[test]
pub fn test_carbon_output_host() {
  let provider = StatsProvider::new(Some(String::from("5")), Vec::new());
  let scope = provider.collector().scope("prefix");
  let ctr1 = scope.counter("counter");
  ctr1.inc();
  let ctr2 = scope.counter("another_counter");
  ctr2.inc();
  ctr2.inc();

  let output = provider.carbon_output(123_456);
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "prefix.another_counter.count 2 123456 source=\"5\"\nprefix.counter.count 1 123456 \
     source=\"5\"\nstats.label_tracker.label_overflow.count 0 123456 source=\"5\"\n"
  );
}

#[test]
pub fn test_carbon_output_timestamp() {
  let provider = StatsProvider::new(None, Vec::new());
  let scope = provider.collector().scope("prefix");
  let ctr1 = scope.counter("counter");
  ctr1.inc();
  let ctr2 = scope.counter("another_counter");
  ctr2.inc();
  ctr2.inc();

  let output = provider.carbon_output(44_444_444);
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "prefix.another_counter.count 2 44444444 source=\"0\"\nprefix.counter.count 1 44444444 \
     source=\"0\"\nstats.label_tracker.label_overflow.count 0 44444444 source=\"0\"\n"
  );
}

#[test]
pub fn test_statsd_output() {
  let provider = StatsProvider::new(None, vec![]);
  let scope = provider.collector().scope("prefix");
  let ctr1 = scope.counter("counter");
  ctr1.inc();
  let ctr2 = scope.counter("another_counter");
  ctr2.inc();
  ctr2.inc();

  let output = provider.statsd_output();
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "prefix.another_counter.count:2|c#source:0\nprefix.counter.count:1|c#source:0\nstats.\
     label_tracker.label_overflow.count:0|c#source:0\n"
  );
}

#[test]
pub fn test_statsd_output_scope_labels() {
  let provider = StatsProvider::new(None, vec![]);
  let scope = provider.collector().scope("prefix");
  let ctr1 = scope.counter("counter");
  ctr1.inc();
  let mut labels = HashMap::new();
  labels.insert(String::from("region"), String::from("IAD"));
  labels.insert(String::from("hello"), String::from("world"));
  let ctr2 = scope
    .scope_with_labels("", labels)
    .counter("another_counter");
  ctr2.inc();
  ctr2.inc();

  let output = provider.statsd_output();
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "prefix.another_counter.count:2|c#source:0,hello:world,region:IAD\nprefix.counter.count:1|c#\
     source:0\nstats.label_tracker.label_overflow.count:0|c#source:0\n"
  );
}
