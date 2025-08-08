// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use futures_util::Future;
use std::any::Any;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock, Weak};
use tokio::sync::Mutex;

pub struct SingletonHandle {
  slot: usize,
}

impl Default for SingletonHandle {
  fn default() -> Self {
    static NEXT_SLOT: OnceLock<AtomicUsize> = OnceLock::new();

    Self {
      slot: NEXT_SLOT
        .get_or_init(AtomicUsize::default)
        .fetch_add(1, Ordering::SeqCst),
    }
  }
}

#[derive(Default)]
pub struct SingletonManager {
  singletons: Mutex<Vec<Option<Weak<dyn Any + Send + Sync>>>>,
}

impl SingletonManager {
  pub async fn get_or_init<T: Send + Sync + 'static, E>(
    &self,
    handle: &SingletonHandle,
    init_func: impl Future<Output = Result<Arc<T>, E>> + Send,
  ) -> Result<Arc<T>, E> {
    let mut singletons = self.singletons.lock().await;
    if singletons.len() < handle.slot + 1 {
      singletons.resize(handle.slot + 1, None);
    }
    if let Some(singleton) = &singletons[handle.slot]
      && let Some(singleton) = singleton.upgrade()
    {
      return Ok(singleton.clone().downcast().unwrap());
    }

    let to_insert = init_func.await?;
    singletons[handle.slot] = Some(Arc::downgrade(
      &(to_insert.clone() as Arc<dyn Any + Send + Sync>),
    ));
    Ok(to_insert)
  }
}
