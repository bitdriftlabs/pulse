// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::ElisionProcessor;
use crate::admin::server::{Admin, AdminHandlerHandle};
use crate::pipeline::processor::internode::InternodeProcessor;
use axum::body::Body;
use axum::extract::Request;
use axum::response::Response;
use futures::FutureExt;
use parking_lot::Mutex;
use pulse_common::singleton::{SingletonHandle, SingletonManager};
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, OnceLock, Weak};
use unwrap_infallible::UnwrapInfallible;

//
// Inner
//

#[derive(Default)]
struct Inner {
  elisions: Mutex<Vec<Weak<ElisionProcessor>>>,
  internodes: Mutex<Vec<Weak<InternodeProcessor>>>,
}

impl Inner {
  // Get the last elided timestamp from elision processors and optionally internode processors.
  async fn get_last_elided(&self, metric: &str, internode: bool) -> anyhow::Result<Option<u64>> {
    // TODO(mattklein123): The use of weak/retain here could cause unbound growth if there is a lot
    // of change without calling. We can figure out an inline delete but seems not worth it now.
    let mut res = None;
    self.elisions.lock().retain(|e| e.upgrade().is_some());
    for elision in &*self.elisions.lock() {
      if let Some(elision) = elision.upgrade() {
        res = res.max(elision.get_last_elided(metric)?);
      }
    }

    if internode {
      self.internodes.lock().retain(|i| i.upgrade().is_some());
      let internodes = self.internodes.lock().clone();
      for internode in internodes {
        if let Some(internode) = internode.upgrade() {
          res = res.max(internode.get_last_elided(metric).await?);
        }
      }
    }

    Ok(res)
  }

  // The handler for the /last_elided admin endpoint.
  async fn last_elided_handler(&self, req: Request) -> Response {
    let query: HashMap<String, String> = req.uri().query().map_or_else(HashMap::default, |q| {
      url::form_urlencoded::parse(q.as_bytes())
        .into_owned()
        .collect()
    });

    let metric = match query.get("metric") {
      Some(m) if !m.is_empty() => m,
      _ => {
        return Response::builder()
          .status(400)
          .body(Body::from("missing metric parameter"))
          .unwrap();
      },
    };

    match self.get_last_elided(metric, true).await {
      Ok(last_elided) => Response::builder()
        .status(200)
        .body(Body::from(last_elided.unwrap_or_default().to_string()))
        .unwrap(),
      Err(e) => Response::builder()
        .status(500)
        .body(Body::from(format!("error fetching last elided: {e}")))
        .unwrap(),
    }
  }
}

//
// GetLastElided
//

// Global handler for computing the last elided timestamp for a given metric. Installs the
// /last_elided admin handler which then consults elision and internode processors to find the
// timestamp.
pub struct GetLastElided {
  inner: Arc<Inner>,
  _admin_handler_handle: Box<dyn AdminHandlerHandle>,
}

impl GetLastElided {
  // Get the singleton instance.
  async fn get_instance(singleton_manager: &SingletonManager, admin: &dyn Admin) -> Arc<Self> {
    static HANDLE: OnceLock<SingletonHandle> = OnceLock::new();
    let handle = HANDLE.get_or_init(SingletonHandle::default);

    singleton_manager
      .get_or_init(handle, async {
        let inner: Arc<Inner> = Arc::default();
        let cloned_inner = inner.clone();

        let admin_handler_handle = admin
          .register_handler(
            "/last_elided",
            Box::new(move |request| {
              let cloned_inner = cloned_inner.clone();
              async move { cloned_inner.last_elided_handler(request).await }.boxed()
            }),
          )
          .unwrap();

        Ok::<_, Infallible>(Arc::new(Self {
          inner,
          _admin_handler_handle: admin_handler_handle,
        }))
      })
      .await
      .unwrap_infallible()
  }

  // Get the last elided timestamp from elision processors and optionally internode processors.
  pub async fn get_last_elided(
    &self,
    metric: &str,
    internode: bool,
  ) -> anyhow::Result<Option<u64>> {
    self.inner.get_last_elided(metric, internode).await
  }

  // Register an elision processor.
  #[must_use]
  pub async fn register_elision(
    processor: Arc<ElisionProcessor>,
    singleton_manager: &SingletonManager,
    admin: &dyn Admin,
  ) -> Arc<Self> {
    let get_last_elided = Self::get_instance(singleton_manager, admin).await;
    get_last_elided
      .inner
      .elisions
      .lock()
      .push(Arc::downgrade(&processor));
    get_last_elided
  }

  // Register an internode processor.
  #[must_use]
  pub async fn register_internode(
    processor: Arc<InternodeProcessor>,
    singleton_manager: &SingletonManager,
    admin: &dyn Admin,
  ) -> Arc<Self> {
    let get_last_elided = Self::get_instance(singleton_manager, admin).await;
    get_last_elided
      .inner
      .internodes
      .lock()
      .push(Arc::downgrade(&processor));
    get_last_elided
  }
}
