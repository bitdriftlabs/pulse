// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./shard_map_test.rs"]
mod shard_map_test;

use core::fmt;
use log::{debug, warn};
use protobuf::Chars;
use pulse_protobuf::protos::pulse::config::processor::v1::internode::InternodeConfig;
use pulse_protobuf::protos::pulse::config::processor::v1::internode::internode_config::NodeConfig;
use regex::Regex;
use std::collections::HashSet;
use std::hash::{BuildHasher, Hash};
use xxhash_rust::xxh64::Xxh64Builder;

pub const SELF_NODE_KEY: &str = "self";

#[derive(Debug, Clone)]
pub struct Node<T> {
  pub id: String,
  pub address: String,
  pub is_self: bool,
  pub inner: Option<T>,
}

impl<T> Node<T> {
  pub fn new(id: String, address: String, inner: Option<T>) -> Self {
    let network_address: Regex = Regex::new(r"^.+:\d+$").unwrap();
    if !network_address.is_match(address.as_str()) {
      warn!("Unable to parse node address: {address}");
    }
    Self {
      id,
      address,
      is_self: false,
      inner,
    }
  }
}

#[derive(Clone)]
pub struct ShardMap<T> {
  // All nodes including self, sorted by id
  pub nodes: Vec<Node<T>>,
  hash_builder: Xxh64Builder,
}

impl<T: fmt::Debug> fmt::Debug for ShardMap<T> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("ShardMap")
      .field("nodes", &self.nodes)
      .finish_non_exhaustive()
  }
}

impl<T> ShardMap<T> {
  // Pick node for the given hashable (metric).
  // If the option is empty the node is you!
  pub fn pick_node<H: Hash + ?Sized>(&self, metric: &H) -> (usize, Option<&T>) {
    if self.nodes.is_empty() {
      return (0, None);
    }

    let hash = self.hash_builder.hash_one(metric);
    let index = usize::try_from(hash).unwrap() % (self.nodes.len());
    if let Some(node) = self.nodes.get(index) {
      return if node.is_self {
        (index, None)
      } else {
        (index, node.inner.as_ref())
      };
    }
    warn!("Picked out of bounds node, defaulting to self.");
    (0, None)
  }

  #[must_use]
  pub fn peer_list(&self) -> Vec<Chars> {
    self
      .nodes
      .iter()
      .map(|n| {
        if n.is_self {
          SELF_NODE_KEY.into()
        } else {
          n.address.clone().into()
        }
      })
      .collect()
  }
}

#[derive(thiserror::Error, Debug)]
pub enum InternodeError {
  #[error(
    "total_nodes count invalid (expected {0}, found {1}, including self). Did you include this \
     node in the count?"
  )]
  TotalNodesInvalid(usize, usize),
  #[error("total_nodes requires a minimum of 2 nodes")]
  TotalNodesMinimum,
  #[error("config specified with no peer nodes")]
  NoPeersSpecified,
  #[error("config contains duplicate node id")]
  DuplicateId,
}

pub fn validate_shardmap_config(cfg: &InternodeConfig) -> Result<(), InternodeError> {
  // Total nodes is optional, only validated if present
  if let Some(total_nodes) = cfg.total_nodes {
    if total_nodes < 2 {
      return Err(InternodeError::TotalNodesMinimum);
    } else if u32::try_from(cfg.nodes.len()).unwrap() != total_nodes - 1 {
      return Err(InternodeError::TotalNodesInvalid(
        cfg.nodes.len() + 1,
        total_nodes as usize,
      ));
    }
  }

  if cfg.nodes.is_empty() {
    return Err(InternodeError::NoPeersSpecified);
  }

  // Check for duplicate ids
  let mut node_ids: HashSet<String> = HashSet::new();
  node_ids.insert(cfg.this_node_id.to_string());
  for node_cfg in &cfg.nodes {
    if node_ids.contains(node_cfg.node_id.as_str()) {
      return Err(InternodeError::DuplicateId);
    }
    node_ids.insert(node_cfg.node_id.to_string());
  }

  Ok(())
}

pub fn shardmap_from_config<T, F>(
  cfg: &InternodeConfig,
  mut factory: F,
) -> anyhow::Result<ShardMap<T>>
where
  F: FnMut(&NodeConfig) -> T,
  T: Clone + Send + std::fmt::Debug,
{
  validate_shardmap_config(cfg)?;

  let mut nodes: Vec<Node<T>> = Vec::with_capacity(cfg.nodes.len() + 1);
  nodes.push(Node {
    id: cfg.this_node_id.to_string(),
    address: "127.0.0.1:3199".parse().unwrap(), // This won't actually get used
    is_self: true,
    inner: None,
  });
  for node_cfg in &cfg.nodes {
    let inner = factory(node_cfg);
    nodes.push(Node::new(
      node_cfg.node_id.to_string(),
      node_cfg.address.to_string(),
      Some(inner),
    ));
  }

  // Sorting shards by ID so they're consistent across servers
  nodes.sort_by(|s1, s2| s1.id.cmp(&s2.id));
  debug!("shardmap load: {nodes:?}");
  Ok(ShardMap {
    nodes,
    hash_builder: Xxh64Builder::new(0),
  })
}

#[must_use]
pub fn peer_list_is_match(our_peers: &[Chars], their_peers: &[Chars], peer: &str) -> bool {
  if our_peers.len() != their_peers.len() {
    return false;
  }

  let num_mismatched = our_peers
    .iter()
    .zip(their_peers)
    .fold(0, |acc, (us, them)| {
      if us == them && &**us != SELF_NODE_KEY && &**them != SELF_NODE_KEY {
        // TODO: should really resolve ip and separate port to allow for canonical matching
        return acc; // straight peer match
      }
      if &**us == SELF_NODE_KEY && &**them != SELF_NODE_KEY {
        return acc; // this is our address, we don't do further validation on our listen
      }
      if &**them == SELF_NODE_KEY && &**us == peer {
        return acc; // this is their address
      }

      warn!("Internode issue: {peer} reported node {them} but we have {us}");
      acc + 1
    });

  num_mismatched == 0
}
