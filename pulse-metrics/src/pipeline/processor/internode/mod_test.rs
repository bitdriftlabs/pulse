// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::pipeline::processor::PipelineProcessor;
use crate::pipeline::processor::internode::InternodeProcessor;
use crate::test::{make_abs_counter, processor_factory_context_for_test};
use bd_test_helpers::make_mut;
use prometheus::labels;
use pulse_common::bind_resolver::make_reuse_port_tcp_socket;
use pulse_protobuf::protos::pulse::config::processor::v1::internode::InternodeConfig;
use pulse_protobuf::protos::pulse::config::processor::v1::internode::internode_config::{
  NodeConfig,
  RequestPolicy,
};

#[tokio::test]
async fn overflow() {
  let (mut helper, context) = processor_factory_context_for_test();
  let bound_socket = make_reuse_port_tcp_socket("127.0.0.1:0").await.unwrap();
  make_mut(&mut helper.bind_resolver)
    .expect_resolve_tcp()
    .return_once(move |_| Ok(bound_socket));
  let processor = InternodeProcessor::new(
    InternodeConfig {
      nodes: vec![NodeConfig {
        node_id: "node_1".into(),
        address: "fake".into(),
        ..Default::default()
      }],
      request_policy: Some(RequestPolicy {
        max_pending_requests: Some(0),
        ..Default::default()
      })
      .into(),
      ..Default::default()
    },
    context,
  )
  .await
  .unwrap();
  processor
    .recv_samples(vec![make_abs_counter("foo", &[], 0, 1.0)])
    .await;

  helper
    .stats_helper
    .assert_counter_eq(1, "processor:internode_overflow", &labels! {});
}
