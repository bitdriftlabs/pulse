// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::pipeline::metric_cache::{CachedMetric, MetricCache};
use crate::pipeline::processor::PipelineProcessor;
use crate::pipeline::processor::mutate::MutateProcessor;
use crate::protos::metric::{EditableParsedMetric, ParsedMetric, TagValue};
use crate::test::{
  ProcessorFactoryContextHelper,
  make_abs_counter,
  make_abs_counter_with_metadata,
  processor_factory_context_for_test,
};
use assert_matches::assert_matches;
use bd_server_stats::stats::Collector;
use bd_test_helpers::make_mut;
use pretty_assertions::assert_eq;
use prometheus::labels;
use pulse_common::k8s::NodeInfo;
use pulse_common::k8s::test::make_node_info;
use pulse_common::metadata::{Metadata, PodMetadata};
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
async fn modify_all_tags() {
  let mut helper = Helper::new(
    r#"
new_tags = {}
for_each(.tags) -> |key, value| {
  new_tags = set!(new_tags, [key], replace(value, r'[.=:]', "_"))
}
.tags = new_tags
    "#,
  );

  helper
    .expect_send_and_receive(
      make_abs_counter(
        "kube_job_status_failed",
        &[("foo", "1.1.1"), ("bar", "ok"), ("baz", "a=b")],
        0,
        1.0,
      ),
      make_abs_counter(
        "kube_job_status_failed",
        &[("bar", "ok"), ("baz", "a_b"), ("foo", "1_1_1")],
        0,
        1.0,
      ),
    )
    .await;
}

#[tokio::test]
async fn node_only() {
  let mut helper = Helper::new(
    r"
.tags.node = %k8s.node.name
.tags.node_ip = %k8s.node.ip
.tags.node_label = %k8s.node.labels.k1
.tags.node_annotation = %k8s.node.annotations.a1
    ",
  );

  helper
    .expect_send_and_receive(
      make_abs_counter_with_metadata(
        "test:name",
        &[("a", "b")],
        0,
        1.0,
        Metadata::new(
          Some(&NodeInfo {
            name: "node_name".to_string(),
            ip: "node_ip".to_string(),
            labels: btreemap!("k1" => "v1"),
            annotations: btreemap!("a1" => "b1"),
            kubelet_port: 123,
          }),
          None,
          None,
        ),
      ),
      make_abs_counter_with_metadata(
        "test:name",
        &[
          ("a", "b"),
          ("node", "node_name"),
          ("node_annotation", "b1"),
          ("node_ip", "node_ip"),
          ("node_label", "v1"),
        ],
        0,
        1.0,
        Metadata::new(
          Some(&NodeInfo {
            name: "node_name".to_string(),
            ip: "node_ip".to_string(),
            labels: btreemap!("k1" => "v1"),
            annotations: btreemap!("a1" => "b1"),
            kubelet_port: 123,
          }),
          None,
          None,
        ),
      ),
    )
    .await;
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
        Some(&make_node_info()),
        Some(PodMetadata {
          namespace: "default",
          pod_name: "podA",
          pod_ip: "podAIP",
          pod_labels: &btreemap!(),
          pod_annotations: &btreemap!(),
          service: Some("kube_state_metrics"),
        }),
        None,
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
          Some(&make_node_info()),
          Some(PodMetadata {
            namespace: "default",
            pod_name: "podA",
            pod_ip: "podAIP",
            pod_labels: &btreemap!(),
            pod_annotations: &btreemap!(),
            service: Some("kube_state_metrics"),
          }),
          None,
        ),
      ),
      make_abs_counter_with_metadata(
        "kube_state_metrics:kube_job_status_failed",
        &[("namespace", "default"), ("pod", "podA")],
        0,
        1.0,
        Metadata::new(
          Some(&make_node_info()),
          Some(PodMetadata {
            namespace: "default",
            pod_name: "podA",
            pod_ip: "podAIP",
            pod_labels: &btreemap!(),
            pod_annotations: &btreemap!(),
            service: Some("kube_state_metrics"),
          }),
          None,
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
          Some(&make_node_info()),
          Some(PodMetadata {
            namespace: "default",
            pod_name: "podA",
            pod_ip: "podAIP",
            pod_labels: &btreemap!(),
            pod_annotations: &btreemap!(),
            service: Some("some-service"),
          }),
          None,
        ),
      ),
      make_abs_counter_with_metadata(
        "test:name",
        &[("namespace", "a"), ("pod", "b")],
        0,
        1.0,
        Metadata::new(
          Some(&make_node_info()),
          Some(PodMetadata {
            namespace: "default",
            pod_name: "podA",
            pod_ip: "podAIP",
            pod_labels: &btreemap!(),
            pod_annotations: &btreemap!(),
            service: Some("some-service"),
          }),
          None,
        ),
      ),
    )
    .await;
}

#[tokio::test]
async fn test_transformation_kubernetes_service_name() {
  let mut helper = Helper::new(
    r#"
.name = join!([get_env_var!("TEST"), ":", replace!(%k8s.service.name, "-", "_"), ":", .name])
.tags.pod = %k8s.pod.name
.tags.namespace = %k8s.namespace
.tags.foo_key = %k8s.pod.labels.label_a
.tags.bar_key = %k8s.pod.annotations.annotation_b
    "#,
  );

  unsafe {
    std::env::set_var("TEST", "prod");
  }

  helper
    .expect_send_and_receive(
      make_abs_counter_with_metadata(
        "test:name",
        &[("a", "b")],
        0,
        1.0,
        Metadata::new(
          Some(&make_node_info()),
          Some(PodMetadata {
            namespace: "default",
            pod_name: "podA",
            pod_ip: "podAIP",
            pod_labels: &btreemap!("label_a" => "foo"),
            pod_annotations: &btreemap!("annotation_b" => "bar"),
            service: Some("some-service"),
          }),
          None,
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
          Some(&make_node_info()),
          Some(PodMetadata {
            namespace: "default",
            pod_name: "podA",
            pod_ip: "podAIP",
            pod_labels: &btreemap!("label_a" => "foo"),
            pod_annotations: &btreemap!("annotation_b" => "bar"),
            service: Some("some-service"),
          }),
          None,
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
.tags.pod_ip = %k8s.pod.ip
.tags.node = %k8s.node.name
.tags.node_ip = %k8s.node.ip
.tags.namespace = %k8s.namespace
.tags.scrape_address = %prom.scrape.address
.tags.mtype = .mtype
    "#,
  );

  unsafe {
    std::env::set_var("TEST", "prod");
  }

  helper
    .expect_send_and_receive(
      make_abs_counter_with_metadata(
        "test:pod_name",
        &[],
        0,
        1.0,
        Metadata::new(
          Some(&make_node_info()),
          Some(PodMetadata {
            namespace: "default",
            pod_name: "podA",
            pod_ip: "podAIP",
            pod_labels: &btreemap!(),
            pod_annotations: &btreemap!(),
            service: None,
          }),
          Some("1.2.3.4:8000".to_string()),
        ),
      ),
      make_abs_counter_with_metadata(
        "prod:default:test:pod_name",
        &[
          ("namespace", "default"),
          ("pod", "podA"),
          ("pod_ip", "podAIP"),
          ("node", "node_name"),
          ("node_ip", "node_ip"),
          ("scrape_address", "1.2.3.4:8000"),
          ("mtype", "counter"),
        ],
        0,
        1.0,
        Metadata::new(
          Some(&make_node_info()),
          Some(PodMetadata {
            namespace: "default",
            pod_name: "podA",
            pod_ip: "podAIP",
            pod_labels: &btreemap!(),
            pod_annotations: &btreemap!(),
            service: None,
          }),
          Some("1.2.3.4:8000".to_string()),
        ),
      ),
    )
    .await;
}

#[tokio::test]
async fn test_deletion() {
  let mut helper = Helper::new(
    r"
del(.tags.key1)
.tags.key3 = del(.tags.key2)
    ",
  );

  helper
    .expect_send_and_receive(
      make_abs_counter_with_metadata(
        "test:foo",
        &[("key1", "value1"), ("key2", "value2")],
        0,
        1.0,
        Metadata::new(
          Some(&make_node_info()),
          Some(PodMetadata {
            namespace: "default",
            pod_name: "podA",
            pod_ip: "podAIP",
            pod_labels: &btreemap!(),
            pod_annotations: &btreemap!(),
            service: None,
          }),
          None,
        ),
      ),
      make_abs_counter_with_metadata(
        "test:foo",
        &[("key3", "value2")],
        0,
        1.0,
        Metadata::new(
          Some(&make_node_info()),
          Some(PodMetadata {
            namespace: "default",
            pod_name: "podA",
            pod_ip: "podAIP",
            pod_labels: &btreemap!(),
            pod_annotations: &btreemap!(),
            service: None,
          }),
          None,
        ),
      ),
    )
    .await;
}

#[tokio::test]
async fn vrl_return() {
  let mut helper = Helper::new(
    r#"
if exists(.tags.key1) {
  return true
}
.tags.key2 = "hello"
    "#,
  );

  helper
    .expect_send_and_receive(
      make_abs_counter_with_metadata(
        "test:foo",
        &[("key1", "value1")],
        0,
        1.0,
        Metadata::new(
          Some(&make_node_info()),
          Some(PodMetadata {
            namespace: "default",
            pod_name: "podA",
            pod_ip: "podAIP",
            pod_labels: &btreemap!(),
            pod_annotations: &btreemap!(),
            service: None,
          }),
          None,
        ),
      ),
      make_abs_counter_with_metadata(
        "test:foo",
        &[("key1", "value1")],
        0,
        1.0,
        Metadata::new(
          Some(&make_node_info()),
          Some(PodMetadata {
            namespace: "default",
            pod_name: "podA",
            pod_ip: "podAIP",
            pod_labels: &btreemap!(),
            pod_annotations: &btreemap!(),
            service: None,
          }),
          None,
        ),
      ),
    )
    .await;
}

#[test]
fn editable_parsed_metric() {
  let metric_cache = MetricCache::new(&Collector::default().scope("test"), None);

  {
    let mut metric = make_abs_counter("prod:some_service:test:name", &[], 0, 1.0);
    metric.initialize_cache(&metric_cache);
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
    assert_matches!(metric.cached_metric(), CachedMetric::NotInitialized);
  }

  {
    let mut metric = make_abs_counter("prod:some_service:test:name", &[], 0, 1.0);
    metric.initialize_cache(&metric_cache);
    let mut editable_metric = EditableParsedMetric::new(&mut metric);
    editable_metric.change_name("foo".into());
    drop(editable_metric);
    assert_eq!(metric, make_abs_counter("foo", &[], 0, 1.0));
    assert_matches!(metric.cached_metric(), CachedMetric::NotInitialized);
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

#[test]
fn test_mutate_metrics_deletion() {
  let metric_cache = MetricCache::new(&Collector::default().scope("test"), None);

  {
    let mut metric = make_abs_counter("prod:some_service:test:name", &[], 0, 1.0);
    metric.initialize_cache(&metric_cache);
    let mut editable_metric = EditableParsedMetric::new(&mut metric);
    assert!(editable_metric.delete_tag(b"pod").is_none());
    drop(editable_metric);
    assert_eq!(
      metric,
      make_abs_counter("prod:some_service:test:name", &[], 0, 1.0)
    );
    assert_matches!(metric.cached_metric(), CachedMetric::Loaded(_, _));
  }

  {
    let mut metric = make_abs_counter("prod:some_service:test:name", &[("pod", "podA")], 0, 1.0);
    metric.initialize_cache(&metric_cache);
    let mut editable_metric = EditableParsedMetric::new(&mut metric);
    assert_eq!("podA", editable_metric.delete_tag(b"pod").unwrap());
    assert!(editable_metric.find_tag(b"pod").is_none());
    drop(editable_metric);
    assert_eq!(
      metric,
      make_abs_counter("prod:some_service:test:name", &[], 0, 1.0)
    );
    assert_matches!(metric.cached_metric(), CachedMetric::NotInitialized);
  }

  {
    let mut metric = make_abs_counter("prod:some_service:test:name", &[("pod", "podA")], 0, 1.0);
    let mut editable_metric = EditableParsedMetric::new(&mut metric);
    assert_eq!("podA", editable_metric.delete_tag(b"pod").unwrap());
    editable_metric.add_or_change_tag(TagValue {
      tag: "pod".into(),
      value: "podB".into(),
    });
    assert_eq!("podB", editable_metric.find_tag(b"pod").unwrap().value);
    drop(editable_metric);
    assert_eq!(
      metric,
      make_abs_counter("prod:some_service:test:name", &[("pod", "podB")], 0, 1.0)
    );
  }

  {
    let mut metric = make_abs_counter(
      "prod:some_service:test:name",
      &[("key1", "value1"), ("key2", "value2"), ("key3", "value3")],
      0,
      1.0,
    );
    let mut editable_metric = EditableParsedMetric::new(&mut metric);
    assert_eq!("value2", editable_metric.delete_tag(b"key2").unwrap());
    editable_metric.add_or_change_tag(TagValue {
      tag: "pod".into(),
      value: "podA".into(),
    });
    editable_metric.add_or_change_tag(TagValue {
      tag: "namespace".into(),
      value: "namespaceA".into(),
    });
    assert_eq!("podA", editable_metric.delete_tag(b"pod").unwrap());
    assert!(editable_metric.find_tag(b"pod").is_none());
    drop(editable_metric);
    assert_eq!(
      metric,
      make_abs_counter(
        "prod:some_service:test:name",
        &[
          ("key1", "value1"),
          ("key3", "value3"),
          ("namespace", "namespaceA")
        ],
        0,
        1.0
      )
    );
  }
}
