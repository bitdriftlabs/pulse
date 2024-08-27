// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::*;

#[test]
pub fn test_counter() {
  let collector = Collector::default();
  let scope = collector.scope("prefix");
  let ctr1 = scope.counter("counter");
  ctr1.inc();
  let ctr2 = scope.counter("counter");
  // Ensure we have the same counter object
  assert_eq!(ctr2.get(), 1);
  ctr2.inc();
  assert_eq!(ctr1.get(), 2);
}

#[test]
pub fn test_counter_with_scope_labels() {
  let collector = Collector::default();
  let mut labels = HashMap::new();
  labels.insert(String::from("hello"), String::from("there"));

  let base_scope = collector.scope("prefix");
  let label_scope = base_scope.scope_with_labels("", labels.clone());
  let ctr1 = label_scope.counter("counter");
  ctr1.inc();
  let ctr2 = base_scope.counter_with_labels("counter", labels.clone());
  // Ensure we have the same counter object
  assert_eq!(ctr2.get(), 1);
  ctr2.inc();
  assert_eq!(ctr1.get(), 2);
}

#[test]
pub fn test_counter_with_overlapping_scope_labels() {
  let collector = Collector::default();
  let mut labels = HashMap::new();
  labels.insert(String::from("hello"), String::from("there"));
  labels.insert(String::from("yummy"), String::from("cheese"));

  let mut labels2 = HashMap::new();
  labels2.insert(String::from("hello"), String::from("goodbye"));

  let base_scope = collector.scope("prefix");
  let label_scope = base_scope.scope_with_labels("", labels.clone());
  // This should override the value for the "hello" label.
  let ctr1 = label_scope.counter_with_labels("counter", labels2);
  ctr1.inc();

  labels.insert(String::from("hello"), String::from("goodbye"));
  let ctr2 = base_scope.counter_with_labels("counter", labels.clone());

  // Ensure we have the same counter object
  assert_eq!(ctr2.get(), 1);
  ctr2.inc();
  assert_eq!(ctr1.get(), 2);
}

#[test]
pub fn test_counter_with_same_labels() {
  let collector = Collector::default();
  let scope = collector.scope("prefix");
  let mut labels1 = HashMap::new();
  labels1.insert(String::from("response"), String::from("success"));
  let ctr1 = scope.counter_with_labels("counter", labels1);
  ctr1.inc();

  let mut labels2 = HashMap::new();
  labels2.insert(String::from("response"), String::from("success"));
  let ctr2 = scope.counter_with_labels("counter", labels2);
  // Ensure we have the same counter object
  assert_eq!(ctr2.get(), 1);
  ctr2.inc();
  assert_eq!(ctr1.get(), 2);
}

#[test]
pub fn test_counter_with_different_labels() {
  let collector = Collector::default();
  let scope = collector.scope("prefix");
  let mut labels1 = HashMap::new();
  labels1.insert(String::from("response"), String::from("success"));
  let ctr1 = scope.counter_with_labels("counter", labels1);
  ctr1.inc();
  assert_eq!(ctr1.get(), 1);

  let mut labels2 = HashMap::new();
  labels2.insert(String::from("response"), String::from("failure"));
  let ctr2 = scope.counter_with_labels("counter", labels2);
  // Ensure we have different counter objects
  assert_eq!(ctr2.get(), 0);
  ctr2.inc();
  assert_eq!(ctr2.get(), 1);
}

#[test]
pub fn test_gauge() {
  let collector = Collector::default();
  let scope = collector.scope("prefix");
  let ctr1 = scope.gauge("gauge");
  ctr1.set(12);
  let ctr2 = scope.gauge("gauge");
  // Ensure we have the same gauge object
  assert_eq!(ctr2.get(), 12);
  ctr2.set(13);
  assert_eq!(ctr1.get(), 13);
}

#[test]
pub fn test_gauges_with_same_labels() {
  let collector = Collector::default();
  let mut labels = HashMap::new();
  labels.insert(String::from("yummy"), String::from("cheese"));

  let scope = collector.scope("prefix");
  let ctr1 = scope.gauge_with_labels("gauge", labels.clone());
  ctr1.set(12);

  let scope2 = scope.scope_with_labels("", labels.clone());
  let ctr2 = scope2.gauge("gauge");
  // Ensure we have the same gauge object
  assert_eq!(ctr2.get(), 12);
  ctr2.set(13);
  assert_eq!(ctr1.get(), 13);
}

#[test]
pub fn test_gauges_with_different_labels() {
  let collector = Collector::default();
  let mut labels = HashMap::new();
  labels.insert(String::from("yummy"), String::from("cheese"));

  let scope = collector.scope("prefix");
  let ctr1 = scope.gauge_with_labels("gauge", labels);
  ctr1.set(12);

  let mut labels2 = HashMap::new();
  labels2.insert(String::from("yummy"), String::from("chocolate"));
  let ctr2 = scope.gauge_with_labels("gauge", labels2);
  // Ensure we have different gauge objects
  assert_eq!(ctr2.get(), 0);
  ctr2.set(13);

  assert_eq!(ctr1.get(), 12);
  assert_eq!(ctr2.get(), 13);
}

#[test]
pub fn test_histogram() {
  let collector = Collector::default();
  let scope = collector.scope("prefix");

  let h1 = scope.histogram("histogram", None);
  let timer = h1.start_timer();
  assert_eq!(0, h1.num_samples());
  std::mem::drop(timer);
  assert_eq!(1, h1.num_samples());

  let h2 = scope.histogram("histogram", None);
  // Ensure we have the same histogram object
  assert_eq!(1, h2.num_samples());

  let mut timer2 = h2.start_timer();
  timer2.cancel();
  timer2.cancel(); // Test double calls
  std::mem::drop(timer2); // Make sure we don't record
  assert_eq!(1, h2.num_samples());
}

#[test]
pub fn test_histogram_observe() {
  let collector = Collector::default();
  let scope = collector.scope("prefix");

  let h1 = scope.histogram("histogram", None);
  h1.observe(0.1);
  assert_eq!(1, h1.num_samples());
}

#[test]
pub fn test_carbon_output() {
  let collector = Collector::default();
  let scope = collector.scope("prefix");
  let ctr1 = scope.counter("counter");
  ctr1.inc();
  let ctr2 = scope.counter("another_counter");
  ctr2.inc();
  ctr2.inc();

  let output = collector.carbon_output(123_456);
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "prefix.another_counter.count 2 123456 source=\"0\"\nprefix.counter.count 1 123456 \
     source=\"0\"\n"
  );
}

#[test]
pub fn test_carbon_output_labels() {
  let collector = Collector::default();
  let scope = collector.scope("prefix");
  let ctr1 = scope.counter("counter");
  ctr1.inc();
  let mut labels = HashMap::new();
  labels.insert(String::from("region"), String::from("IAD"));
  labels.insert(String::from("hello"), String::from("world"));
  let ctr2 = scope.counter_with_labels("another_counter", labels);
  ctr2.inc();
  ctr2.inc();

  let output = collector.carbon_output(123_456);
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "prefix.another_counter.count 2 123456 source=\"0\" hello=\"world\" \
     region=\"IAD\"\nprefix.counter.count 1 123456 source=\"0\"\n"
  );
}

#[test]
pub fn test_carbon_output_scope_labels() {
  let collector = Collector::default();
  let scope = collector.scope("prefix");
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

  let output = collector.carbon_output(123_456);
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "prefix.another_counter.count 2 123456 source=\"0\" hello=\"world\" \
     region=\"IAD\"\nprefix.counter.count 1 123456 source=\"0\"\n"
  );
}

#[test]
pub fn test_carbon_output_labels_empty_name() {
  let collector = Collector::default();
  let scope = collector.scope("prefix");
  let ctr1 = scope.counter("counter");
  ctr1.inc();
  let mut labels = HashMap::new();
  labels.insert(String::from("region"), String::from("IAD"));
  labels.insert(String::from("hello"), String::from("world"));
  let ctr2 = scope.counter_with_labels("", labels);
  ctr2.inc();
  ctr2.inc();

  let output = collector.carbon_output(123_456);
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "prefix.count 2 123456 source=\"0\" hello=\"world\" region=\"IAD\"\nprefix.counter.count 1 \
     123456 source=\"0\"\n"
  );
}

#[test]
pub fn test_carbon_output_gauge() {
  let collector = Collector::default();
  let scope = collector.scope("prefix");
  let c = scope.counter("counter");
  c.inc();
  let g = scope.gauge("some_gauge");
  g.set(2);

  let output = collector.carbon_output(123_456);
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "prefix.counter.count 1 123456 source=\"0\"\nprefix.some_gauge.gauge 2 123456 source=\"0\"\n"
  );
}

#[test]
pub fn test_carbon_output_histogram() {
  let collector = Collector::default();
  let scope = collector.scope("prefix");
  let h1 = scope.histogram("histogram", None);
  h1.observe(0.1);

  let output = collector.carbon_output(123_456);
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
     123456 source=\"0\" le=\"+Inf\"\n"
  );
}

#[test]
pub fn test_carbon_output_histogram_custom_buckets() {
  let collector = Collector::default();
  let scope = collector.scope("prefix");
  let h1 = scope.histogram("histogram", Some(vec![0.01, 0.05]));
  h1.observe(0.1);

  let output = collector.carbon_output(123_456);
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "prefix.histogram.count 1 123456 source=\"0\"\nprefix.histogram.sum 0.1 123456 \
     source=\"0\"\nprefix.histogram.bucket 0 123456 source=\"0\" \
     le=\"0.01\"\nprefix.histogram.bucket 0 123456 source=\"0\" \
     le=\"0.05\"\nprefix.histogram.bucket 1 123456 source=\"0\" le=\"+Inf\"\n"
  );
}

#[test]
pub fn test_carbon_output_host() {
  let collector = Collector::new(Some(String::from("5")), None, Vec::new());
  let scope = collector.scope("prefix");
  let ctr1 = scope.counter("counter");
  ctr1.inc();
  let ctr2 = scope.counter("another_counter");
  ctr2.inc();
  ctr2.inc();

  let output = collector.carbon_output(123_456);
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "prefix.another_counter.count 2 123456 source=\"5\"\nprefix.counter.count 1 123456 \
     source=\"5\"\n"
  );
}

#[test]
pub fn test_carbon_output_prefix() {
  let collector = Collector::new(
    None,
    Some(String::from("production.app.pulse")),
    vec![(String::from("hostkey"), String::from("hostname1"))],
  );
  let scope = collector.scope("prefix");
  let ctr1 = scope.counter("counter");
  ctr1.inc();
  let ctr2 = scope.counter("another_counter");
  ctr2.inc();
  ctr2.inc();

  let output = collector.carbon_output(123_456);
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "production.app.pulse.prefix.another_counter.count 2 123456 source=\"0\" \
     hostkey=\"hostname1\"\nproduction.app.pulse.prefix.counter.count 1 123456 source=\"0\" \
     hostkey=\"hostname1\"\n"
  );
}

#[test]
pub fn test_carbon_output_prefix_and_host() {
  let collector = Collector::new(
    Some(String::from("6")),
    Some(String::from("production.app.mmeproxy")),
    vec![
      (String::from("k1"), String::from("v1")),
      (String::from("k2"), String::from("v2")),
    ],
  );
  let scope = collector.scope("prefix");
  let ctr1 = scope.counter("counter");
  ctr1.inc();
  let ctr2 = scope.counter("another_counter");
  ctr2.inc();
  ctr2.inc();

  let output = collector.carbon_output(123_456);
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "production.app.mmeproxy.prefix.another_counter.count 2 123456 source=\"6\" k1=\"v1\" \
     k2=\"v2\"\nproduction.app.mmeproxy.prefix.counter.count 1 123456 source=\"6\" k1=\"v1\" \
     k2=\"v2\"\n"
  );
}

#[test]
pub fn test_carbon_output_timestamp() {
  let collector = Collector::new(None, None, Vec::new());
  let scope = collector.scope("prefix");
  let ctr1 = scope.counter("counter");
  ctr1.inc();
  let ctr2 = scope.counter("another_counter");
  ctr2.inc();
  ctr2.inc();

  let output = collector.carbon_output(44_444_444);
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "prefix.another_counter.count 2 44444444 source=\"0\"\nprefix.counter.count 1 44444444 \
     source=\"0\"\n"
  );
}

#[test]
pub fn test_generate_metric_id_same() {
  let mut labels1 = HashMap::new();
  labels1.insert(String::from("hello"), String::from("there"));
  let id1 = generate_metric_id(String::from("counter"), labels1);

  let mut labels2 = HashMap::new();
  labels2.insert(String::from("hello"), String::from("there"));
  let id2 = generate_metric_id(String::from("counter"), labels2);

  assert_eq!(id1, id2);
}

#[test]
pub fn test_generate_metric_id_unordered_tags() {
  let mut labels1 = HashMap::new();
  labels1.insert(String::from("hello"), String::from("there"));
  labels1.insert(String::from("yummy"), String::from("cheese"));
  let id1 = generate_metric_id(String::from("counter"), labels1);

  let mut labels2 = HashMap::new();
  labels2.insert(String::from("yummy"), String::from("cheese"));
  labels2.insert(String::from("hello"), String::from("there"));
  let id2 = generate_metric_id(String::from("counter"), labels2);

  assert_eq!(id1, id2);
}

#[test]
pub fn test_generate_metric_id_different() {
  let mut labels1 = HashMap::new();
  labels1.insert(String::from("hello"), String::from("there"));
  let id1 = generate_metric_id(String::from("counter"), labels1);

  let mut labels2 = HashMap::new();
  labels2.insert(String::from("hello"), String::from("goodbye"));
  let id2 = generate_metric_id(String::from("counter"), labels2);

  assert_ne!(id1, id2);
}

#[test]
pub fn test_statsd_output() {
  let collector = Collector::default();
  let scope = collector.scope("prefix");
  let ctr1 = scope.counter("counter");
  ctr1.inc();
  let ctr2 = scope.counter("another_counter");
  ctr2.inc();
  ctr2.inc();

  let output = collector.statsd_output();
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "prefix.another_counter.count:2|c#source:0\nprefix.counter.count:1|c#source:0\n"
  );
}

#[test]
pub fn test_statsd_output_scope_labels() {
  let collector = Collector::default();
  let scope = collector.scope("prefix");
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

  let output = collector.statsd_output();
  let output_str = String::from_utf8(output).unwrap();
  assert_eq!(
    output_str,
    "prefix.another_counter.count:2|c#source:0,hello:world,region:IAD\nprefix.counter.count:1|c#\
     source:0\n"
  );
}
