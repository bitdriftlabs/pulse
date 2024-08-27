// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::filter::{MetricFilter, MetricFilterDecision};
use crate::admin::server::{Admin, AdminHandlerHandle};
use crate::file_watcher::{get_file_watcher, DynamicFileWatcher};
use crate::protos::metric::{Metric, MetricType, MetricValue};
use axum::response::Response;
use bd_server_stats::stats::Scope;
use bd_shutdown::ComponentShutdown;
use bd_time::TimeDurationExt;
use bytes::Bytes;
use futures::{Future, FutureExt};
use http::StatusCode;
use log::{info, warn};
use parking_lot::Mutex;
use prometheus::{IntCounter, IntGauge};
use pulse_common::singleton::{SingletonHandle, SingletonManager};
use pulse_protobuf::protos::pulse::config::common::v1::file_watcher::FileSourceConfig;
use std::convert::Infallible;
use std::sync::{Arc, OnceLock, Weak};
use time::ext::NumericalDuration;
use tokio::task::JoinHandle;
use unwrap_infallible::UnwrapInfallible;

//
// PollFilterStats
//

#[derive(Clone)]
pub struct PollFilterStats {
  pub poll_fail: IntCounter,
  pub load_total: IntCounter,
  pub load_fail: IntCounter,
  pub set_size: IntGauge,
}

impl PollFilterStats {
  #[must_use]
  pub fn new_with_scope(scope: &Scope) -> Self {
    Self {
      poll_fail: scope.counter("poll_fail"),
      load_total: scope.counter("load_total"),
      load_fail: scope.counter("load_fail"),
      set_size: scope.gauge("set_size"),
    }
  }
}

//
// PollFilterProvider
//

#[async_trait::async_trait]
pub trait PollFilterProvider: Send + Sync {
  // Update the provider with new file bytes.
  async fn update(&self, file: Bytes) -> Result<(), anyhow::Error>;
  // Determine whether the current file contents match a metric name.
  fn is_match(&self, metric_name: &[u8]) -> bool;
  // Dump the provider for the /dump_poll_filter admin endpoint.
  // TODO(mattklein123): We could have this return a stream if we want to avoid allocating space
  // for the entire contents again. Putting multiple streams together into the final streaming
  // response is not trivial so this is deferred for now.
  fn dump(&self) -> Vec<u8>;
  // Get the number of items in the backing set.
  fn set_size(&self) -> usize;
}

//
// PollFilterAdminHandler
//

struct PollFilterAdminHandler {
  filters: Arc<Mutex<Vec<Weak<PollFilter>>>>,
  _admin_handler_handle: Box<dyn AdminHandlerHandle>,
}

impl PollFilterAdminHandler {
  #[must_use]
  async fn register(
    poll_filter: Arc<PollFilter>,
    singleton_manager: &SingletonManager,
    admin: &dyn Admin,
  ) -> Arc<Self> {
    static HANDLE: OnceLock<SingletonHandle> = OnceLock::new();
    let handle = HANDLE.get_or_init(SingletonHandle::default);

    let handler = singleton_manager
      .get_or_init(handle, async {
        let filters: Arc<Mutex<Vec<Weak<PollFilter>>>> = Arc::default();
        let cloned_filters = filters.clone();

        let admin_handler_handle = admin
          .register_handler(
            "/dump_poll_filter",
            Box::new(move |_| {
              let cloned_filters = cloned_filters.clone();
              async move { Self::handler(cloned_filters).await }.boxed()
            }),
          )
          .unwrap();

        Ok::<_, Infallible>(Arc::new(Self {
          filters,
          _admin_handler_handle: admin_handler_handle,
        }))
      })
      .await
      .unwrap_infallible();

    handler.filters.lock().push(Arc::downgrade(&poll_filter));
    handler
  }

  async fn handler(filters: Arc<Mutex<Vec<Weak<PollFilter>>>>) -> Response {
    let mut output = Vec::new();
    // TODO(mattklein123): The use of weak/retain here could cause unbound growth if there is a lot
    // of change without calling. We can figure out an inline delete but seems not worth it now.
    filters.lock().retain(|filter| {
      filter.upgrade().map_or(false, |filter| {
        output.extend(format!("filter name: {}\n", filter.name).as_bytes());
        output.extend(filter.provider.dump());
        output.push(b'\n');
        true
      })
    });

    Response::builder()
      .status(StatusCode::OK)
      .body(output.into())
      .unwrap()
  }
}

//
// PollFilter
//

// Wrapper for a filter that polls a data source, loads the data, and then does matches against it.
pub struct PollFilter {
  name: String,
  match_decision: MetricFilterDecision,
  provider: Box<dyn PollFilterProvider>,
  prom: bool,
  file_poll_handle: Mutex<Option<JoinHandle<()>>>,
  admin_handler: Mutex<Option<Arc<PollFilterAdminHandler>>>,
}

impl PollFilter {
  #[cfg(test)]
  pub fn new_for_test(
    provider: impl PollFilterProvider + 'static,
    match_decision: MetricFilterDecision,
    prom: bool,
    name: String,
  ) -> Self {
    Self {
      name,
      match_decision,
      provider: Box::new(provider),
      prom,
      file_poll_handle: Mutex::default(),
      admin_handler: Mutex::default(),
    }
  }

  /// Constructs a new PollFilter from a config.
  pub async fn try_from_config<
    T: PollFilterProvider + 'static,
    F: Future<Output = Result<T, anyhow::Error>>,
  >(
    provider_factory: impl Fn(Bytes) -> F,
    scope: &Scope,
    source: FileSourceConfig,
    prom: bool,
    name: &str,
    match_decision: MetricFilterDecision,
    shutdown: ComponentShutdown,
    singleton_manager: &SingletonManager,
    admin: Arc<dyn Admin>,
  ) -> anyhow::Result<Arc<Self>> {
    info!("loading file from {}", source);
    let scope = &scope.scope("poll");
    let stats = PollFilterStats::new_with_scope(scope);
    stats.load_total.inc();

    let (file_watcher, file) = get_file_watcher(source.clone(), shutdown.clone())
      .await
      .map_err(|e| {
        stats.load_fail.inc();
        e
      })?;
    let provider = provider_factory(file).await.map_err(|e| {
      stats.load_fail.inc();
      e
    })?;
    info!("successfully loaded file");
    stats.set_size.set(provider.set_size().try_into().unwrap());

    let filter = Arc::new(Self {
      name: name.to_string(),
      match_decision,
      provider: Box::new(provider),
      prom,
      file_poll_handle: Mutex::default(),
      admin_handler: Mutex::default(),
    });

    // Spawn a task to poll for file changes.
    let cloned_filter = filter.clone();
    let handle = tokio::spawn(async move {
      cloned_filter
        .poll(file_watcher, source, stats.clone(), shutdown)
        .await;
    });
    *filter.file_poll_handle.lock() = Some(handle);

    let handler =
      PollFilterAdminHandler::register(filter.clone(), singleton_manager, admin.as_ref()).await;
    *filter.admin_handler.lock() = Some(handler);

    Ok(filter)
  }

  pub fn decide(&self, metric: &Metric) -> MetricFilterDecision {
    let metric_name = if self.prom {
      metric.get_id().prom_name()
    } else {
      metric.get_id().name().clone()
    };

    if self.provider.is_match(metric_name.as_ref()) {
      self.match_decision
    } else {
      MetricFilterDecision::NotCovered
    }
  }

  async fn poll(
    &self,
    mut file_watcher: DynamicFileWatcher,
    source: FileSourceConfig,
    stats: PollFilterStats,
    mut shutdown: ComponentShutdown,
  ) {
    let shutdown = shutdown.cancelled();
    tokio::pin!(shutdown);

    loop {
      let file = tokio::select! {
        () = &mut shutdown => break,
        file = file_watcher.wait_until_modified() => file,
      };

      let file = match file {
        Ok(file) => file,
        Err(e) => {
          stats.poll_fail.inc();
          warn!("failed to poll watcher: {}", e);
          // Avoid spin loops, even if the underlying impl sleeps.
          1.seconds().sleep().await;
          continue;
        },
      };

      info!("changed detected, reloading from {}", source);
      stats.load_total.inc();
      match self.provider.update(file).await {
        Ok(()) => {
          info!("successfully updated filter");
          stats
            .set_size
            .set(self.provider.set_size().try_into().unwrap());
        },
        Err(e) => {
          warn!("failed to update filter: {}", e);
          stats.load_fail.inc();
        },
      }
    }
  }
}

impl Drop for PollFilter {
  fn drop(&mut self) {
    if let Some(handle) = self.file_poll_handle.lock().as_ref() {
      handle.abort();
    }
  }
}

impl MetricFilter for PollFilter {
  fn decide(
    &self,
    metric: &Metric,
    _: &Option<MetricValue>,
    _: Option<MetricType>,
  ) -> MetricFilterDecision {
    self.decide(metric)
  }

  fn name(&self) -> &str {
    &self.name
  }

  fn match_decision(&self) -> MetricFilterDecision {
    self.match_decision
  }
}
