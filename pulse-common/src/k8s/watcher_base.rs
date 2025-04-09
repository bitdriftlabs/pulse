// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use async_trait::async_trait;
use backoff::backoff::Backoff;
use backoff::{ExponentialBackoff, ExponentialBackoffBuilder};
use bd_shutdown::ComponentShutdown;
use futures_util::{TryStreamExt, pin_mut};
use kube::runtime::watcher::{self, ListSemantic};
use kube::{Api, Resource};
use serde::de::DeserializeOwned;
use std::fmt::Debug;
use time::ext::NumericalStdDuration;
use tokio::sync::oneshot;

#[async_trait]
pub trait ResourceWatchCallbacks<T>: Send {
  async fn apply(&mut self, resource: T);
  async fn delete(&mut self, resource: T);
  async fn init_apply(&mut self, resource: T);
  async fn init_done(&mut self);
}

pub struct WatcherBase {}

impl WatcherBase {
  pub async fn create<T: Resource + Clone + DeserializeOwned + Debug + Send + 'static>(
    name: String,
    api: Api<T>,
    field_selector: Option<String>,
    mut callbacks: impl ResourceWatchCallbacks<T> + 'static,
    mut shutdown: ComponentShutdown,
  ) {
    // Note that we unset page size because the Rust library won't set resourceVersion=0 when
    // page size is set because apparently K8s ignores it. See:
    // https://github.com/kubernetes/kubernetes/issues/118394
    let watcher = watcher::watcher(
      api,
      watcher::Config {
        field_selector,
        list_semantic: ListSemantic::Any,
        page_size: None,
        ..Default::default()
      },
    );

    let (initial_sync_tx, initial_sync_rx) = oneshot::channel();
    let mut backoff = Self::make_k8s_backoff();

    let cloned_name = name.clone();
    tokio::spawn(async move {
      pin_mut!(watcher);
      let mut initial_sync_tx = Some(initial_sync_tx);
      loop {
        let update = tokio::select! {
          () = shutdown.cancelled() => {
            log::info!("{name}: shutting down resource watcher");
            break;
          },
          update = watcher.try_next() => update
        };

        let Some(update) = Self::process_resource_update(update, &mut backoff).await else {
          continue;
        };

        match update {
          watcher::Event::Apply(resource) => {
            log::debug!("{name}: resource apply");
            callbacks.apply(resource).await;
          },
          watcher::Event::Delete(resource) => callbacks.delete(resource).await,
          watcher::Event::Init => {
            log::info!("{name}: starting resource resync");
          },
          watcher::Event::InitApply(resource) => {
            log::debug!("{name}: resource init apply");
            callbacks.init_apply(resource).await;
          },
          watcher::Event::InitDone => {
            callbacks.init_done().await;
            log::info!("{name}: resource resync complete");
            if let Some(initial_sync_tx) = initial_sync_tx.take() {
              let _ignored = initial_sync_tx.send(());
            }
          },
        }
      }
    });

    let _ignored = initial_sync_rx.await;
    log::info!("{cloned_name}: initial resource sync complete");
  }

  fn make_k8s_backoff() -> ExponentialBackoff {
    // This matches the k8s client which says that it matches the Go client. This is done manually
    // as there appears to be a bug in the k8s client where it keeps resetting if the failure is
    // during initial sync.
    ExponentialBackoffBuilder::new()
      .with_initial_interval(800.std_milliseconds())
      .with_max_interval(30.std_seconds())
      .with_max_elapsed_time(None)
      .with_multiplier(2.0)
      .build()
  }

  async fn process_resource_update<T>(
    result: watcher::Result<Option<watcher::Event<T>>>,
    backoff: &mut ExponentialBackoff,
  ) -> Option<watcher::Event<T>> {
    match result {
      Ok(Some(update)) => {
        if !matches!(update, watcher::Event::Init) {
          // The library will emit the Event::Init message in the case of a failure and the start of
          // resync. We do not want to reset in this case, but reset in all other cases.
          backoff.reset();
        }

        Some(update)
      },
      // TODO(snowp): Would this ever happen?
      Ok(None) => None,
      Err(e) => {
        log::warn!("Error watching resource, backing off: {e}");
        tokio::time::sleep(backoff.next_backoff().unwrap()).await;
        None
      },
    }
  }
}
