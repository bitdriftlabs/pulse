// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::pipeline::processor::mutate::MutateProcessor;
use crate::pipeline::processor::PipelineProcessor;
use crate::protos::metric::{EditableParsedMetric, ParsedMetric, TagValue};
use crate::test::{
  make_abs_counter,
  make_abs_counter_with_metadata,
  processor_factory_context_for_test,
  ProcessorFactoryContextHelper,
};
use bd_test_helpers::make_mut;
use pretty_assertions::assert_eq;
use prometheus::labels;
use pulse_common::metadata::Metadata;
use pulse_protobuf::protos::pulse::config::processor::v1::mutate::MutateConfig;
use std::sync::Arc;
use vrl::btreemap;

// TODO(mattklein123): The use of env vars in these tests needs to be scoped/randomized as any
// env var change will effect the entire test process which may be run concurrently.

struct Helper {
  helper: ProcessorFactoryContextHelper,
  processor: Arc<MutateProcessor>,
}

impl Helper {
  fn new(vrl_program: &'static str) -> Self {
    let config = MutateConfig {
      vrl_program: vrl_program.into(),
      ..Default::default()
    };

    let (helper, factory) = processor_factory_context_for_test();
    let processor = Arc::new(MutateProcessor::new(&config, factory).unwrap());
    Self { helper, processor }
  }

  async fn expect_send_and_receive(&mut self, send: ParsedMetric, receive: ParsedMetric) {
    make_mut(&mut self.helper.dispatcher)
      .expect_send()
      .times(1)
      .return_once(|samples| {
        assert_eq!(vec![receive], samples);
      });
    self.processor.clone().recv_samples(vec![send]).await;
  }
}

#[tokio::test]
async fn drop_metrics_with_abort() {
  let mut helper = Helper::new(
    r#"
if string!(%k8s.service.name) == "kube_state_metrics" &&
   !match_any(.name, [
    r'^kube_node_status_condition$',
    r'^kube_pod_container_status_restarts_total$',
    r'^kube_pod_container_status_last_terminated_reason$',
    r'^kube_job_status_failed$',
    r'^kube_job_status_succeeded$',
  ]) {
  abort
}

.name = join!([%k8s.service.name, ":", .name])
.tags.pod = %k8s.pod.name
.tags.namespace = %k8s.namespace
    "#,
  );

  helper
    .processor
    .clone()
    .recv_samples(vec![make_abs_counter_with_metadata(
      "test:name",
      &[("a", "b")],
      0,
      1.0,
      Metadata::new(
        "default",
        "podA",
        &btreemap!(),
        &btreemap!(),
        Some("kube_state_metrics"),
      ),
    )])
    .await;
  helper
    .helper
    .stats_helper
    .assert_counter_eq(1, "processor:drop_abort", &labels! {});
  helper
    .expect_send_and_receive(
      make_abs_counter_with_metadata(
        "kube_job_status_failed",
        &[],
        0,
        1.0,
        Metadata::new(
          "default",
          "podA",
          &btreemap!(),
          &btreemap!(),
          Some("kube_state_metrics"),
        ),
      ),
      make_abs_counter_with_metadata(
        "kube_state_metrics:kube_job_status_failed",
        &[("namespace", "default"), ("pod", "podA")],
        0,
        1.0,
        Metadata::new(
          "default",
          "podA",
          &btreemap!(),
          &btreemap!(),
          Some("kube_state_metrics"),
        ),
      ),
    )
    .await;
  helper
    .processor
    .recv_samples(vec![make_abs_counter("test:name", &[], 0, 1.0)])
    .await;
  helper
    .helper
    .stats_helper
    .assert_counter_eq(1, "processor:drop_error", &labels! {});
}

#[tokio::test]
async fn dont_change_existing_tag() {
  let mut helper = Helper::new(
    r"
if is_null(.tags.pod) {
  .tags.pod = %k8s.pod.name
}
if is_null(.tags.namespace) {
  .tags.namespace = %k8s.namespace
}
    ",
  );

  helper
    .expect_send_and_receive(
      make_abs_counter_with_metadata(
        "test:name",
        &[("namespace", "a"), ("pod", "b")],
        0,
        1.0,
        Metadata::new(
          "default",
          "podA",
          &btreemap!(),
          &btreemap!(),
          Some("some-service"),
        ),
      ),
      make_abs_counter_with_metadata(
        "test:name",
        &[("namespace", "a"), ("pod", "b")],
        0,
        1.0,
        Metadata::new(
          "default",
          "podA",
          &btreemap!(),
          &btreemap!(),
          Some("some-service"),
        ),
      ),
    )
    .await;
}

#[tokio::test]
async fn test_transformation_kubernetes_service_name() {
  let mut helper = Helper::new(
    r#"
.name = join!([get_env_var!("TEST"), ":", %k8s.service.name, ":", .name])
.tags.pod = %k8s.pod.name
.tags.namespace = %k8s.namespace
.tags.foo_key = %k8s.pod.labels.label_a
.tags.bar_key = %k8s.pod.annotations.annotation_b
    "#,
  );

  std::env::set_var("TEST", "prod");

  helper
    .expect_send_and_receive(
      make_abs_counter_with_metadata(
        "test:name",
        &[("a", "b")],
        0,
        1.0,
        Metadata::new(
          "default",
          "podA",
          &btreemap!("label_a" => "foo"),
          &btreemap!("annotation_b" => "bar"),
          Some("some-service"),
        ),
      ),
      make_abs_counter_with_metadata(
        "prod:some_service:test:name",
        &[
          ("a", "b"),
          ("bar_key", "bar"),
          ("foo_key", "foo"),
          ("namespace", "default"),
          ("pod", "podA"),
        ],
        0,
        1.0,
        Metadata::new(
          "default",
          "podA",
          &btreemap!("label_a" => "foo"),
          &btreemap!("annotation_b" => "bar"),
          Some("some-service"),
        ),
      ),
    )
    .await;
}

#[tokio::test]
async fn test_transformation_kubernetes_namespace() {
  let mut helper = Helper::new(
    r#"
.name = join!([get_env_var!("TEST"), ":", %k8s.namespace, ":", .name])
.tags.pod = %k8s.pod.name
.tags.namespace = %k8s.namespace
    "#,
  );

  std::env::set_var("TEST", "prod");

  helper
    .expect_send_and_receive(
      make_abs_counter_with_metadata(
        "test:pod_name",
        &[],
        0,
        1.0,
        Metadata::new("default", "podA", &btreemap!(), &btreemap!(), None),
      ),
      make_abs_counter_with_metadata(
        "prod:default:test:pod_name",
        &[("namespace", "default"), ("pod", "podA")],
        0,
        1.0,
        Metadata::new("default", "podA", &btreemap!(), &btreemap!(), None),
      ),
    )
    .await;
}

#[test]
fn editable_parsed_metric() {
  {
    let mut metric = make_abs_counter("prod:some_service:test:name", &[], 0, 1.0);
    let mut editable_metric = EditableParsedMetric::new(&mut metric);
    assert!(editable_metric.find_tag(b"hello").is_none());
    editable_metric.add_or_change_tag(TagValue {
      tag: "hello".into(),
      value: "world".into(),
    });
    assert_eq!("world", editable_metric.find_tag(b"hello").unwrap().value);
    editable_metric.add_or_change_tag(TagValue {
      tag: "hello".into(),
      value: "world2".into(),
    });
    assert_eq!("world2", editable_metric.find_tag(b"hello").unwrap().value);
    editable_metric.add_or_change_tag(TagValue {
      tag: "a".into(),
      value: "b".into(),
    });
    assert_eq!("world2", editable_metric.find_tag(b"hello").unwrap().value);
    assert_eq!("b", editable_metric.find_tag(b"a").unwrap().value);
    drop(editable_metric);
    assert_eq!(
      metric,
      make_abs_counter(
        "prod:some_service:test:name",
        &[("a", "b"), ("hello", "world2")],
        0,
        1.0
      )
    );
  }

  {
    let mut metric = make_abs_counter(
      "prod:some_service:test:name",
      &[("d", "d_value"), ("c", "c_value"), ("b", "b_value")],
      0,
      1.0,
    );
    let mut editable_metric = EditableParsedMetric::new(&mut metric);
    assert_eq!("b_value", editable_metric.find_tag(b"b").unwrap().value);
    assert_eq!("c_value", editable_metric.find_tag(b"c").unwrap().value);
    assert_eq!("d_value", editable_metric.find_tag(b"d").unwrap().value);
    assert!(editable_metric.find_tag(b"a").is_none());
    editable_metric.add_or_change_tag(TagValue {
      tag: "a".into(),
      value: "a_value".into(),
    });
    assert_eq!("a_value", editable_metric.find_tag(b"a").unwrap().value);
    editable_metric.add_or_change_tag(TagValue {
      tag: "z".into(),
      value: "z_value".into(),
    });
    assert_eq!("z_value", editable_metric.find_tag(b"z").unwrap().value);
    assert_eq!("a_value", editable_metric.find_tag(b"a").unwrap().value);
    drop(editable_metric);
    assert_eq!(
      metric,
      make_abs_counter(
        "prod:some_service:test:name",
        &[
          ("a", "a_value"),
          ("b", "b_value"),
          ("c", "c_value"),
          ("d", "d_value"),
          ("z", "z_value"),
        ],
        0,
        1.0
      )
    );
  }
}

#[test]
fn test_mutate_metrics_new_tag() {
  let mut metric = make_abs_counter("prod:some_service:test:name", &[("pod", "podA")], 0, 1.0);
  let mut editable_metric = EditableParsedMetric::new(&mut metric);
  editable_metric.add_or_change_tag(TagValue {
    tag: "namespace".into(),
    value: "default".into(),
  });
  drop(editable_metric);
  assert_eq!(
    metric,
    make_abs_counter(
      "prod:some_service:test:name",
      &[("namespace", "default"), ("pod", "podA")],
      0,
      1.0
    )
  );
}

#[test]
fn test_mutate_metrics_existing_tag() {
  let mut metric = make_abs_counter("prod:some_service:test:name", &[("pod", "podA")], 0, 1.0);
  let mut editable_metric = EditableParsedMetric::new(&mut metric);
  editable_metric.add_or_change_tag(TagValue {
    tag: "pod".into(),
    value: "podB".into(),
  });
  drop(editable_metric);
  assert_eq!(
    metric,
    make_abs_counter("prod:some_service:test:name", &[("pod", "podB")], 0, 1.0)
  );
}