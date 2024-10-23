// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

pub mod admin;
pub mod batch;
pub mod clients;
pub mod file_watcher;
pub mod filters;
pub mod lru_map;
pub mod metric_generator;
pub mod pipeline;
pub mod protos;
pub mod reservoir_timer;
pub mod test;
pub mod vrl;

#[cfg(test)]
#[ctor::ctor]
fn test_global_init() {
  use pulse_common::global_initialize;

  global_initialize();
}
