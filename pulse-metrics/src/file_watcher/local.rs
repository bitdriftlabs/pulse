// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./local_test.rs"]
mod local_test;

use super::{FileWatcher, WatchError};
use async_trait::async_trait;
use bytes::Bytes;
use log::warn;
use notify::event::{ModifyKind, RenameMode};
use notify::{EventKind, RecommendedWatcher, Watcher};
use pulse_protobuf::protos::pulse::config::common::v1::file_watcher::LocalFileSourceConfig;
use std::path::Path;
use std::time::Instant;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::watch;

/// A `LocalFileWatcher` is used to track a local resource.
pub struct LocalFileWatcher {
  source: LocalFileSourceConfig,
  reload_rx: watch::Receiver<Instant>,
  _watcher: RecommendedWatcher,
}

// TODO(mattklein123): Merge the notify stuff into shared-core with Loader.
impl LocalFileWatcher {
  // The default fsevents backend only provides "any" and can't merge rename events. This makes it
  // difficult to avoid spurious reloads. This works on linux though with inotify.
  #[cfg(target_os = "macos")]
  const fn rename_mode() -> RenameMode {
    RenameMode::Any
  }

  #[cfg(target_os = "linux")]
  const fn rename_mode() -> RenameMode {
    RenameMode::Both
  }

  pub async fn new(source: LocalFileSourceConfig) -> Result<(Self, Bytes), WatchError> {
    let (reload_tx, reload_rx) = watch::channel(Instant::now());
    let mut watcher =
      notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| match res {
        Ok(event) => match event.kind {
          EventKind::Modify(ModifyKind::Name(mode)) if mode == Self::rename_mode() => {
            let _ = reload_tx.send(Instant::now());
          },
          _ => (),
        },
        Err(e) => {
          warn!("failed to watch local file for changes: {e}");
        },
      })?;

    watcher.watch(
      Path::new(&source.runtime_config.dir),
      notify::RecursiveMode::NonRecursive,
    )?;
    let watcher = Self {
      source,
      reload_rx,
      _watcher: watcher,
    };
    let file = watcher.load_file().await?;

    Ok((watcher, file))
  }

  async fn load_file(&self) -> Result<Bytes, WatchError> {
    let mut bytes = Vec::new();
    File::open(&self.source.runtime_config.file)
      .await?
      .read_to_end(&mut bytes)
      .await?;
    Ok(bytes.into())
  }
}

#[async_trait]
impl FileWatcher for LocalFileWatcher {
  async fn wait_until_modified(&mut self) -> Result<Bytes, WatchError> {
    self.reload_rx.changed().await?;
    self.load_file().await
  }
}
