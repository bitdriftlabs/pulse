// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::{MockRemoteFileWatcherClient, RemoteFileWatcher};
use crate::file_watcher::{FileWatcher, WatchError};
use bd_shutdown::ComponentShutdownTrigger;
use bd_time::{TimeDurationExt, ToProtoDuration};
use bytes::Bytes;
use mockall::predicate;
use pulse_protobuf::protos::pulse::config::common::v1::file_watcher::HttpFileSourceConfig;
use std::time::Duration;
use tempfile::tempdir;
use time::ext::NumericalDuration;
use tokio::time::timeout;

#[tokio::test(start_paused = true)]
async fn lagged_caching() {
  let mut client = MockRemoteFileWatcherClient::new();
  let temp_dir = tempdir().unwrap();
  let shutdown_trigger = ComponentShutdownTrigger::default();

  client
    .expect_get_file()
    .once()
    .with(predicate::eq(None))
    .returning(|_| Ok((Bytes::from_static(b"one"), String::from("1"))));
  client
    .expect_get_file()
    .once()
    .with(predicate::eq(Some("1".to_string())))
    .returning(|_| Ok((Bytes::from_static(b"two"), String::from("2"))));

  // Create a new watcher.
  let (mut file_watcher, _read_file) = RemoteFileWatcher::new(
    client,
    HttpFileSourceConfig {
      interval: 1.seconds().into_proto(),
      cache_path: Some(temp_dir.path().join("test").display().to_string().into()),
      ..Default::default()
    },
    shutdown_trigger.make_shutdown(),
  )
  .await
  .unwrap();

  // Before yielding to the scheduler, make sure the write file loop blocks before doing any writing
  // or receiving messages.
  file_watcher
    .shared_state
    .thread_synchronizer
    .wait_on("write_file_to_cache_loop")
    .await;
  let handle = tokio::spawn(async move {
    file_watcher.wait_until_modified().await.unwrap();
    file_watcher
  });
  // This will cause the modified loop to fetch and then return, and push a new file to be cached.
  1.seconds().sleep().await;
  let file_watcher = handle.await.unwrap();

  // Wait on the file actually being written, and signal the loop, which should hit a lagged error
  // and then loop around and write the next file.
  file_watcher
    .shared_state
    .thread_synchronizer
    .wait_on("wrote_file_to_cache")
    .await;
  file_watcher
    .shared_state
    .thread_synchronizer
    .signal("write_file_to_cache_loop")
    .await;
  file_watcher
    .shared_state
    .thread_synchronizer
    .barrier_on("wrote_file_to_cache")
    .await;
  file_watcher
    .shared_state
    .thread_synchronizer
    .signal("wrote_file_to_cache")
    .await;

  // Now destroy the watcher and shutdown the write loop.
  shutdown_trigger.shutdown().await;
  drop(file_watcher);

  // Create a new writer, and make sure we load the cached file.
  let mut client = MockRemoteFileWatcherClient::new();
  client
    .expect_get_file()
    .once()
    .with(predicate::eq(None))
    .returning(|_| Err(WatchError::Timeout));
  let shutdown_trigger = ComponentShutdownTrigger::default();
  let (_file_watcher, read_file) = RemoteFileWatcher::new(
    client,
    HttpFileSourceConfig {
      interval: 1.seconds().into_proto(),
      cache_path: Some(temp_dir.path().join("test").display().to_string().into()),
      ..Default::default()
    },
    shutdown_trigger.make_shutdown(),
  )
  .await
  .unwrap();
  assert_eq!("two", &read_file);
}

#[tokio::test]
async fn remote_file_watcher() {
  let mut client = MockRemoteFileWatcherClient::new();

  // First call returns the original file
  client
    .expect_get_file()
    .once()
    .with(predicate::eq(None))
    .returning(|_| Ok((Bytes::from_static(b"one"), String::from("1"))));

  // Next 5 calls return not modified
  client
    .expect_get_file()
    .times(5)
    .with(predicate::eq(Some(String::from("1"))))
    .returning(|_| Err(WatchError::ResourceNotModified));

  // Next call returns new file version
  client
    .expect_get_file()
    .once()
    .with(predicate::eq(Some(String::from("1"))))
    .returning(|_| Ok((Bytes::from_static(b"two"), String::from("2"))));

  // Remaining calls return not modified
  client
    .expect_get_file()
    .with(predicate::eq(Some(String::from("2"))))
    .returning(|_| Err(WatchError::ResourceNotModified));

  let shutdown_trigger = ComponentShutdownTrigger::default();
  let (mut file_watcher, read_file) = RemoteFileWatcher::new(
    client,
    HttpFileSourceConfig {
      interval: 1.nanoseconds().into_proto(),
      ..Default::default()
    },
    shutdown_trigger.make_shutdown(),
  )
  .await
  .unwrap();

  // Assert that initial file has correct contents.
  assert_eq!(b"one", read_file.as_ref());

  // Assert that file watcher catches a file update.
  let read_file = timeout(
    Duration::from_millis(100),
    file_watcher.wait_until_modified(),
  )
  .await
  .unwrap()
  .unwrap();

  assert_eq!(b"two", read_file.as_ref());

  // Assert that file watcher blocks when there are no updates.
  timeout(
    Duration::from_millis(100),
    file_watcher.wait_until_modified(),
  )
  .await
  .unwrap_err();
}
