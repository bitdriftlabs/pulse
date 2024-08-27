// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./config_test.rs"]
mod config_test;

use protobuf::MessageFull;
use pulse_common::proto::yaml_to_proto;

pub fn load_from_file<T: MessageFull>(path: &str) -> anyhow::Result<T> {
  let file_contents = std::fs::read_to_string(path)?;
  yaml_to_proto(&file_contents)
}
