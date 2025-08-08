// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use self::local::LocalFileWatcher;
use self::remote::{HttpRemoteFileWatcherClient, RemoteFileWatcher};
use async_trait::async_trait;
use bd_shutdown::ComponentShutdown;
use bytes::Bytes;
use file_watcher::FileSourceConfig;
use file_watcher::file_source_config::File_source_type;
use http::StatusCode;
use pulse_protobuf::protos::pulse::config::common::v1::file_watcher;
use thiserror::Error;
use tokio::sync::watch;

pub mod local;
pub mod remote;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Error)]
pub enum WatchError {
  #[error("failed to set up file watcher: {0}")]
  FileWatch(#[from] notify::Error),
  #[error("http error: {0}")]
  Http(StatusCode),
  #[error("hyper error: {0}")]
  Hyper(#[from] hyper::Error),
  #[error("hyper client error: {0}")]
  HyperClient(#[from] hyper_util::client::legacy::Error),
  #[error("io error: {0}")]
  Io(#[from] std::io::Error),
  #[error("recv error: {0}")]
  Recv(#[from] watch::error::RecvError),
  #[error("resource missing etag")]
  ResourceMissingEtag,
  #[error("resource not modified")]
  ResourceNotModified,
  #[error("watch timeout")]
  Timeout,
}

/// A `FileWatcher` is a generic interface used to track file modifications, whether the resource is
/// stored locally or remotely.
#[async_trait]
pub trait FileWatcher: Send + Sync {
  /// Blocks until the file has been modified. This should track all modifications that occur during
  /// the file watcher's lifetime, not just when this function is executing. Returns the new file
  /// bytes.
  async fn wait_until_modified(&mut self) -> Result<Bytes, WatchError>;
}

pub type DynamicFileWatcher = Box<dyn FileWatcher + Send + Sync + 'static>;

pub async fn get_file_watcher(
  source: FileSourceConfig,
  shutdown: ComponentShutdown,
) -> Result<(DynamicFileWatcher, Bytes), WatchError> {
  match source.file_source_type.expect("pgv") {
    File_source_type::Local(source) => {
      let (watcher, file) = LocalFileWatcher::new(source).await?;
      Ok((Box::new(watcher), file))
    },
    File_source_type::Http(source) => {
      let client = HttpRemoteFileWatcherClient::new(source.clone());
      let (watcher, file) = RemoteFileWatcher::new(client, source, shutdown).await?;
      Ok((Box::new(watcher), file))
    },
  }
}
