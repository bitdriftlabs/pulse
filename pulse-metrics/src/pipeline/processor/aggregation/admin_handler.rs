// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::admin::server::{Admin, AdminHandlerHandle};
use crate::pipeline::metric_cache::MetricKey;
use axum::response::Response;
use futures_util::FutureExt;
use itertools::Itertools;
use parking_lot::Mutex;
use pulse_common::singleton::{SingletonHandle, SingletonManager};
use std::convert::Infallible;
use std::fmt::Write;
use std::sync::{Arc, OnceLock, Weak};
use unwrap_infallible::UnwrapInfallible;

pub type LastAggregatedMetrics = Arc<Mutex<Option<Vec<Arc<MetricKey>>>>>;
type WeakLastAggregatedMetrics = Weak<Mutex<Option<Vec<Arc<MetricKey>>>>>;
type State = Arc<Mutex<Vec<(String, WeakLastAggregatedMetrics)>>>;

pub struct LastAggregated {
  state: State,
  _admin_handler_handle: Box<dyn AdminHandlerHandle>,
}

impl LastAggregated {
  // Get the singleton instance.
  pub async fn get_instance(
    singleton_manager: &SingletonManager,
    admin: &dyn Admin,
    name: String,
    last_aggregated_metrics: LastAggregatedMetrics,
  ) -> Arc<Self> {
    static HANDLE: OnceLock<SingletonHandle> = OnceLock::new();
    let handle = HANDLE.get_or_init(SingletonHandle::default);

    let last_aggregated = singleton_manager
      .get_or_init(handle, async {
        let state: State = Arc::default();
        let cloned_state = state.clone();

        let admin_handler_handle = admin
          .register_handler(
            "/last_aggregation",
            Box::new(move |_request| {
              let state = cloned_state.clone();
              async move { Self::last_aggregated_handler(&state) }.boxed()
            }),
          )
          .unwrap();

        Ok::<_, Infallible>(Arc::new(Self {
          state,
          _admin_handler_handle: admin_handler_handle,
        }))
      })
      .await
      .unwrap_infallible();

    last_aggregated
      .state
      .lock()
      .push((name, Arc::downgrade(&last_aggregated_metrics)));

    last_aggregated
  }

  // Admin handler for /last_aggregation, registered above.
  fn last_aggregated_handler(state: &State) -> Response {
    // TODO(mattklein123): The use of weak/retain here could cause unbound growth if there is a lot
    // of change without calling. We can figure out an inline delete but seems not worth it now.
    let mut response = String::new();

    state.lock().retain(|(name, metrics)| {
      metrics.upgrade().is_some_and(|metrics| {
        if let Some(metrics) = &*metrics.lock() {
          writeln!(&mut response, "dumping filter: {name}").unwrap();
          response.push_str(&metrics.iter().map(|m| m.to_metric_id()).join("\n"));
          response.push('\n');
        }

        true
      })
    });

    Response::builder().body(response.into()).unwrap()
  }
}
