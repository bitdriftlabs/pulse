// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::*;
use matches::assert_matches;
use tokio_test::assert_err;

#[test]
fn test_config_load() {
  let shards = vec![NodeConfig {
    node_id: "some-node-4".into(),
    address: "10.0.0.1:1234".into(),
    ..Default::default()
  }];
  let shard_cfg = InternodeConfig {
    listen: "10.0.0.2:1234".into(),
    this_node_id: "some-node-2".into(),
    total_nodes: Some(2),
    nodes: shards,
    ..Default::default()
  };
  let shardmap = shardmap_from_config(&shard_cfg, |cfg| cfg.address.clone())
    .expect("Unable to convert shardmap config into internal struct");
  assert_eq!(
    shardmap.nodes.first().unwrap().id,
    String::from("some-node-2")
  );
  assert!(shardmap.nodes.first().unwrap().is_self);
  assert_eq!(
    shardmap.nodes.get(1).unwrap().id,
    String::from("some-node-4")
  );
  assert!(!shardmap.nodes.get(1).unwrap().is_self);
  assert_eq!(shardmap.nodes.get(1).unwrap().address, "10.0.0.1:1234");
}

#[test]
fn test_select_shard() {
  let shards = vec![NodeConfig {
    node_id: "4".into(),
    address: "10.0.0.1:1234".into(),
    ..Default::default()
  }];
  let shard_cfg = InternodeConfig {
    listen: "10.0.0.2:1234".into(),
    this_node_id: "2".into(),
    total_nodes: Some(2),
    nodes: shards,
    ..Default::default()
  };
  let shardmap = shardmap_from_config(&shard_cfg, |cfg| cfg.address.to_string())
    .expect("Unable to convert shardmap config into internal struct");

  assert_eq!(shardmap.pick_node("a"), (0, None));
  assert_eq!(shardmap.pick_node("b"), (0, None));
  assert_eq!(
    shardmap.pick_node("c"),
    (1, Some(&"10.0.0.1:1234".to_string()))
  );
  assert_eq!(shardmap.pick_node("d"), (0, None));

  // Now test the inverse (to simulate the other server getting opposite results)
  let shards = vec![NodeConfig {
    node_id: "2".into(),
    address: "10.0.0.1:1234".into(),
    ..Default::default()
  }];
  let shard_cfg = InternodeConfig {
    listen: "10.0.0.2:1234".into(),
    this_node_id: "4".into(),
    total_nodes: Some(2),
    nodes: shards,
    ..Default::default()
  };
  let shardmap = shardmap_from_config(&shard_cfg, |cfg| cfg.address.to_string())
    .expect("Unable to convert shardmap config into internal struct");

  assert_eq!(
    shardmap.pick_node("a"),
    (0, Some(&"10.0.0.1:1234".to_string()))
  );
  assert_eq!(
    shardmap.pick_node("b"),
    (0, Some(&"10.0.0.1:1234".to_string()))
  );
  assert_eq!(shardmap.pick_node("c"), (1, None));
  assert_eq!(
    shardmap.pick_node("d"),
    (0, Some(&"10.0.0.1:1234".to_string()))
  );
}

#[test]
fn test_conf_with_duplicate_ids() {
  let shards = vec![
    NodeConfig {
      node_id: "4".into(),
      address: "10.0.0.1:1234".into(),
      ..Default::default()
    },
    NodeConfig {
      node_id: "4".into(),
      address: "10.0.0.1:1234".into(),
      ..Default::default()
    },
  ];
  let shard_cfg = InternodeConfig {
    listen: "10.0.0.2:1234".into(),
    this_node_id: "2".into(),
    total_nodes: Some(3),
    nodes: shards,
    ..Default::default()
  };
  let shardmap = shardmap_from_config(&shard_cfg, |cfg| cfg.address.clone());
  assert_err!(shardmap);
}

#[test]
fn test_conf_with_incorrect_shard_count() {
  let shards = vec![NodeConfig {
    node_id: "4".into(),
    address: "1234".into(),
    ..Default::default()
  }];
  let shard_cfg = InternodeConfig {
    listen: "10.0.0.2:1234".into(),
    this_node_id: "2".into(),
    total_nodes: Some(3),
    nodes: shards,
    ..Default::default()
  };
  let shardmap = shardmap_from_config(&shard_cfg, |cfg| cfg.address.clone());
  assert_err!(shardmap);
}

#[test]
fn test_conf_with_no_nodes() {
  let shard_cfg = InternodeConfig {
    listen: "10.0.0.2:1234".into(),
    this_node_id: "2".into(),
    total_nodes: None,
    nodes: Vec::new(),
    ..Default::default()
  };
  let shardmap = shardmap_from_config(&shard_cfg, |cfg| cfg.address.clone());
  assert_err!(shardmap);
}

#[test]
fn test_node_match() {
  let peer: &str = "node1-mme:1234";
  let our_map = vec![SELF_NODE_KEY.into(), peer.into(), "node2-mme:1234".into()];
  let their_map = vec![
    "node3-mme:1234".into(),
    SELF_NODE_KEY.into(),
    "node2-mme:1234".into(),
  ];
  assert_matches!(peer_list_is_match(&our_map, &their_map, peer), true);
}

#[test]
fn test_node_match_length_mismatch() {
  let peer: &str = "node1-mme:1234";
  let our_map = vec![SELF_NODE_KEY.into(), peer.into(), "node2-mme:1234".into()];
  let their_map = vec!["node3-mme:1234".into(), SELF_NODE_KEY.into()];
  // Length match fail
  assert_matches!(peer_list_is_match(&our_map, &their_map, peer), false);
}

#[test]
fn test_node_match_naive_match() {
  let peer: &str = "node1-mme:1234";
  let our_map = vec![SELF_NODE_KEY.into(), peer.into(), "node2-mme:1234".into()];
  let their_map = vec![SELF_NODE_KEY.into(), peer.into(), "node2-mme:1234".into()];
  assert_matches!(peer_list_is_match(&our_map, &their_map, peer), false);
}
