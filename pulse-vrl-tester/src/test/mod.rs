// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::run;

#[test]
fn basic_case() {
  let config = r"
test_cases:
- program: |
    .tags.namespace = %k8s.namespace
    return true
  kubernetes_metadata:
    namespace: foo_namespace
    pod_name: foo_pod
  transforms:
  - metric:
      input: foo:1|c
      output: foo:1|c|#namespace:foo_namespace
  - metric:
      input: bar:1|g
      output: bar:1|g|#namespace:foo_namespace
  ";

  run(config, None).unwrap();
}

#[test]
fn abort() {
  let config = r"
  test_cases:
  - program: |
      if !match_any(.name, [
        r'^staging:infra:k8s:.+_staging:clustercapacity:cpu_request:namespace:sum',
        r'^staging:infra:k8s:.+_staging:clustercapacity:memory_request:namespace:sum',
      ]) {
        abort
      }
    transforms:
    - metric:
        input: staging:infra:k8s:compliance_staging:clustercapacity:cpu_request:namespace:sum:1|c
        output: staging:infra:k8s:compliance_staging:clustercapacity:cpu_request:namespace:sum:1|c
    - metric:
        input: foo:1|c
        output: abort
    ";

  run(config, None).unwrap();
}

#[test]
fn failing_case() {
  let config = r"
test_cases:
- program: |
    .tags.namespace = %k8s.namespace
    .tags.pod = %k8s.pod.name
    .tags.a = %k8s.pod.labels.label_a
    .tags.b = %k8s.pod.annotations.annotation_a
  kubernetes_metadata:
    namespace: foo_namespace
    pod_name: foo_pod
    pod_labels:
      label_a: label_a_value
    pod_annotations:
      annotation_a: annotation_a_value
  transforms:
  - metric:
      input: foo:1|c
      output: foo:1|c|#namespace:foo_namespace
  ";

  assert_eq!(run(config, None).unwrap_err().to_string(), "VRL program '.tags.namespace = %k8s.namespace\n.tags.pod = %k8s.pod.name\n.tags.a = %k8s.pod.labels.label_a\n.tags.b = %k8s.pod.annotations.annotation_a\n' failed to transform 'foo:1|c' into 'foo:1|c|#namespace:foo_namespace': \u{1b}[1mDiff\u{1b}[0m \u{1b}[31m< left\u{1b}[0m / \u{1b}[32mright >\u{1b}[0m :\n Metric {\n     id: MetricId {\n         name: b\"foo\",\n         mtype: Some(\n             Counter(\n                 Delta,\n             ),\n         ),\n         tags: [\n             TagValue {\n\u{1b}[32m>                tag: b\"a\",\u{1b}[0m\n\u{1b}[32m>                value: b\"label_a_value\",\u{1b}[0m\n\u{1b}[32m>            },\u{1b}[0m\n\u{1b}[32m>            TagValue {\u{1b}[0m\n\u{1b}[32m>                tag: b\"b\",\u{1b}[0m\n\u{1b}[32m>                value: b\"annotation_a_value\",\u{1b}[0m\n\u{1b}[32m>            },\u{1b}[0m\n\u{1b}[32m>            TagValue {\u{1b}[0m\n                 tag: b\"namespace\",\n                 value: b\"foo_namespace\",\n\u{1b}[32m>            },\u{1b}[0m\n\u{1b}[32m>            TagValue {\u{1b}[0m\n\u{1b}[32m>                tag: b\"pod\",\u{1b}[0m\n\u{1b}[32m>                value: b\"foo_pod\",\u{1b}[0m\n             },\n         ],\n     },\n     sample_rate: None,\n     timestamp: 0,\n     value: Simple(\n         1.0,\n     ),\n }\n");
}
