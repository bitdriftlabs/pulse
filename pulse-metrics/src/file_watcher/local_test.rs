// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::LocalFileWatcher;
use crate::file_watcher::FileWatcher;
use crate::test::FsConfigSwapHelper;
use futures_util::poll;
use pulse_protobuf::protos::pulse::config::common::v1::common::RuntimeConfig;
use pulse_protobuf::protos::pulse::config::common::v1::file_watcher::LocalFileSourceConfig;
use std::task::Poll::{Pending, Ready};

#[tokio::test]
async fn local_file_watcher() {
  let mut helper = FsConfigSwapHelper::new("one.");

  let source = LocalFileSourceConfig {
    runtime_config: Some(RuntimeConfig {
      dir: helper.path().display().to_string().into(),
      file: helper
        .path()
        .join("config.yaml")
        .display()
        .to_string()
        .into(),
      ..Default::default()
    })
    .into(),
    ..Default::default()
  };
  let (mut file_watcher, read_file) = LocalFileWatcher::new(source).await.unwrap();

  // Assert that initial file has correct contents.
  assert_eq!(b"one.", read_file.as_ref());

  // Assert that it blocks before an update occurs.
  let mut changed = file_watcher.wait_until_modified();
  match poll!(changed.as_mut()) {
    Ready(_) => unreachable!(),
    Pending => (),
  }

  // Assert that it catches a file update.
  helper.update_config("one.two.");
  let read_file = changed.await.unwrap();
  assert_eq!(b"one.two.", read_file.as_ref());
}
