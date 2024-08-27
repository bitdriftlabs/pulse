// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::server::{Admin, AdminHandler, AdminHandlerHandle};
use axum::body::Body;
use axum::extract::Request;
use axum::response::Response;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

struct MockAdminHandlerHandleImpl {
  path: String,
  handlers: Arc<Mutex<HashMap<String, AdminHandler>>>,
}

impl AdminHandlerHandle for MockAdminHandlerHandleImpl {}

impl Drop for MockAdminHandlerHandleImpl {
  fn drop(&mut self) {
    let mut handlers = self.handlers.lock();
    assert!(handlers.contains_key(&self.path));
    handlers.remove(&self.path);
  }
}

#[derive(Default)]
pub struct MockAdmin {
  handlers: Arc<Mutex<HashMap<String, AdminHandler>>>,
}

impl MockAdmin {
  pub async fn run(&self, path: &str) -> Response {
    let handler = self.handlers.lock().get(path).unwrap()(Request::new(Body::empty()));
    handler.await
  }
}

impl Admin for MockAdmin {
  fn register_handler(
    &self,
    path: &str,
    handler: AdminHandler,
  ) -> anyhow::Result<Box<dyn AdminHandlerHandle>> {
    let mut handlers = self.handlers.lock();
    assert!(!handlers.contains_key(path));
    handlers.insert(path.to_string(), handler);
    Ok(Box::new(MockAdminHandlerHandleImpl {
      path: path.to_string(),
      handlers: self.handlers.clone(),
    }))
  }
}
