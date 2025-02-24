// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::pipeline::processor::PipelineProcessor;
use crate::pipeline::processor::regex::RegexProcessor;
use crate::test::{make_metric, processor_factory_context_for_test};
use bd_test_helpers::make_mut;
use prometheus::labels;
use pulse_protobuf::protos::pulse::config::processor::v1::regex::RegexConfig;
use std::sync::Arc;

#[tokio::test]
async fn all() {
  let (mut helper, context) = processor_factory_context_for_test();
  let processor = Arc::new(
    RegexProcessor::new(
      &RegexConfig {
        allow: vec!["^hello:.*".into(), "^world:.*".into()],
        deny: vec!["^hello:foo:.*".into()],
        ..Default::default()
      },
      context,
    )
    .unwrap(),
  );
  processor.clone().start().await;

  make_mut(&mut helper.dispatcher)
    .expect_send()
    .times(1)
    .returning(|metrics| {
      assert_eq!(
        metrics,
        vec![
          make_metric("world:foo", &[], 0),
          make_metric("hello:blah", &[], 0),
        ]
      );
    });
  processor
    .clone()
    .recv_samples(vec![
      make_metric("boo", &[], 0),
      make_metric("zoo", &[], 0),
      make_metric("world", &[], 0),
      make_metric("world:foo", &[], 0),
      make_metric("hello:blah", &[], 0),
      make_metric("hello:foo:bar", &[], 0),
    ])
    .await;
  helper
    .stats_helper
    .assert_counter_eq(4, "processor:drop", &labels! {});
}
