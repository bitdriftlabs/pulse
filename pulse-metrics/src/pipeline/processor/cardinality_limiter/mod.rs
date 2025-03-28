// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./mod_test.rs"]
mod mod_test;

use super::{PipelineProcessor, ProcessorFactoryContext};
use crate::pipeline::PipelineDispatch;
use crate::protos::metric::{MetricId, ParsedMetric};
use crate::vrl::ProgramWrapper;
use ahash::{HashMap, HashMapExt};
use async_trait::async_trait;
use bd_log::warn_every;
use bd_shutdown::{ComponentShutdown, ComponentShutdownTriggerHandle};
use bd_time::TimeDurationExt;
use cardinality_limiter_config::per_pod_limit::Override_limit_location;
use cardinality_limiter_config::{GlobalLimit, Limit_type, PerPodLimit};
use cuckoofilter::CuckooFilter;
use parking_lot::{Mutex, RwLock};
use prometheus::IntCounter;
use pulse_common::k8s::pods_info::OwnedPodsInfoSingleton;
use pulse_common::k8s::pods_info::container::PodsInfo;
use pulse_common::metadata::Metadata;
use pulse_common::proto::ProtoDurationToStdDuration;
use pulse_protobuf::protos::pulse::config::processor::v1::cardinality_limiter::{
  CardinalityLimiterConfig,
  cardinality_limiter_config,
};
use std::collections::VecDeque;
use std::hash::Hasher;
use std::sync::Arc;
use time::Duration;
use time::ext::NumericalDuration;
use tokio::time::MissedTickBehavior;
use vrl::core::Value;

//
// Limiter
//

enum LimitResult {
  Ok,
  Drop,
}
trait Limiter: Send + Sync {
  fn limit(&self, metric: &MetricId, metadata: Option<&Arc<Metadata>>) -> LimitResult;
  fn rotate(&self);
}

//
// GlobalLimiter
//

// Wraps multiple windowed filters into a single filter.
struct GlobalLimiter<H> {
  buckets: Mutex<VecDeque<CuckooFilter<H>>>,
  size_limit: usize,
}

impl<H: Hasher + Default + Send> Limiter for GlobalLimiter<H> {
  fn limit(&self, metric: &MetricId, _metadata: Option<&Arc<Metadata>>) -> LimitResult {
    let mut filters = self.buckets.lock();
    if filters[0].contains(metric) {
      log::trace!("metric '{}' might already be in filter", metric);
      LimitResult::Ok
    } else if filters[0].len() >= self.size_limit {
      LimitResult::Drop
    } else {
      for filter in &mut *filters {
        let result = filter.add(metric);
        log::trace!("added metric '{}' to filter: {result:?}", metric);
      }
      LimitResult::Ok
    }
  }

  fn rotate(&self) {
    let new_cuckoo = CuckooFilter::with_capacity(self.size_limit);
    let mut filters = self.buckets.lock();
    filters.pop_front();
    filters.push_back(new_cuckoo);
    log::debug!("first filter has {} element(s) in it", filters[0].len());
  }
}

impl<H: Hasher + Default> GlobalLimiter<H> {
  fn new(config: &GlobalLimit, buckets: usize) -> Self {
    Self {
      buckets: Mutex::new(
        (0 .. buckets)
          .map(|_| CuckooFilter::with_capacity(config.size_limit as usize))
          .collect(),
      ),
      size_limit: config.size_limit as usize,
    }
  }
}

//
// K8sPodLimiterConfig
//

struct K8sPodLimiterConfig {
  default_size_limit: u32,
  vrl_program: Option<ProgramWrapper>,
}

//
// K8sPodLimiter
//

// Sets up per-pod limits where each pod that is dynamically discovered will get its own windowed
// limiter.
type ActivePods<H> = Arc<RwLock<HashMap<String, Arc<GlobalLimiter<H>>>>>;
struct K8sPodLimiter<H> {
  active_pods: ActivePods<H>,
}

impl<H: Hasher + Default + Send> Limiter for K8sPodLimiter<H> {
  fn limit(&self, metric: &MetricId, metadata: Option<&Arc<Metadata>>) -> LimitResult {
    let Some(k8s_namespace_and_pod_name) = metadata.and_then(|m| m.k8s_namespace_and_pod_name())
    else {
      // We assume that if someone configured per-pod limiting they also configured k8s discovery
      // on the inflow which should force metadata to be set. If there is no metadata on the
      // metric just drop.
      log::debug!("dropping metric '{metric}' with no metadata");
      return LimitResult::Drop;
    };

    // Inflows that assign metadata will drop metrics if no k8s lookup is possible, so if we don't
    // have the pod in our map that is due to a small time window where an update hasn't happened
    // yet. In this case we just let the metric through until we get the update.
    self
      .active_pods
      .read()
      .get(k8s_namespace_and_pod_name)
      .map_or(LimitResult::Ok, |limiter| limiter.limit(metric, metadata))
  }

  fn rotate(&self) {
    // Technically it might be better to use independent rotation windows for each discovered pod
    // but that would be more complicated so rotation might happen a bit early for new pods.
    for limiter in self.active_pods.read().values() {
      limiter.rotate();
    }
  }
}

impl<H: Hasher + Default + Send + 'static> K8sPodLimiter<H> {
  fn new(
    config: &PerPodLimit,
    buckets: usize,
    mut k8s_pods_info: OwnedPodsInfoSingleton,
    shutdown: ComponentShutdown,
  ) -> anyhow::Result<Self> {
    let pod_limiter_config = K8sPodLimiterConfig {
      default_size_limit: config.default_size_limit,
      vrl_program: config
        .override_limit_location
        .as_ref()
        .map(|override_limit_location| match override_limit_location {
          Override_limit_location::VrlProgram(vrl_program) => ProgramWrapper::new(vrl_program),
        })
        .transpose()?,
    };

    let active_pods: ActivePods<H> = Arc::default();
    Self::update(
      &active_pods,
      &k8s_pods_info.borrow_and_update(),
      &pod_limiter_config,
      buckets,
    );

    tokio::spawn(Self::update_loop(
      active_pods.clone(),
      pod_limiter_config,
      buckets,
      k8s_pods_info,
      shutdown,
    ));

    Ok(Self { active_pods })
  }

  fn update(
    active_pods: &ActivePods<H>,
    pods_info: &PodsInfo,
    config: &K8sPodLimiterConfig,
    buckets: usize,
  ) {
    let mut new_active_pods = HashMap::new();
    let old_active_pods = active_pods.read();
    for (_, pod) in pods_info.pods() {
      let namespace_and_name = pod.namespace_and_name();
      if let Some(old_limiter) = old_active_pods.get(&namespace_and_name) {
        log::debug!("using existing limiter for '{namespace_and_name}'");
        new_active_pods.insert(namespace_and_name, old_limiter.clone());
      } else {
        let size_limit = config
          .vrl_program
          .as_ref()
          .and_then(
            |vrl_program| match vrl_program.run_with_metadata(Some(&pod.metadata)) {
              Ok(Value::Integer(result)) => Some(result as u32),
              result => {
                warn_every!(
                  1.minutes(),
                  "cardinality VRL program did not return an integer for '{namespace_and_name}': \
                   {:?}",
                  result
                );
                None
              },
            },
          )
          .unwrap_or(config.default_size_limit);

        log::debug!("creating new limiter for '{namespace_and_name}' with size limit {size_limit}");
        new_active_pods.insert(
          namespace_and_name,
          Arc::new(GlobalLimiter::new(
            &GlobalLimit {
              size_limit,
              ..Default::default()
            },
            buckets,
          )),
        );
      }
    }

    drop(old_active_pods);
    log::debug!("active limiters: {}", new_active_pods.len());
    *active_pods.write() = new_active_pods;
  }

  async fn update_loop(
    active_pods: ActivePods<H>,
    config: K8sPodLimiterConfig,
    buckets: usize,
    mut k8s_pods_info: OwnedPodsInfoSingleton,
    mut shutdown: ComponentShutdown,
  ) {
    let shutdown = shutdown.cancelled();
    tokio::pin!(shutdown);

    loop {
      tokio::select! {
        () = &mut shutdown => break,
        () = k8s_pods_info.changed() => ()
      }

      log::debug!("performing pod update");
      Self::update(
        &active_pods,
        &k8s_pods_info.borrow_and_update(),
        &config,
        buckets,
      );
    }

    log::debug!("shutting down pod update loop");
  }
}

//
// CardinalityLimiterProcessor
//

// Cardinality limiter using rotating Cuckoo filter buckets. See the proto config for more
// information.
pub struct CardinalityLimiterProcessor {
  dispatcher: Arc<dyn PipelineDispatch>,
  limiter: Box<dyn Limiter>,
  rotate_after: Duration,
  drop: IntCounter,
  shutdown: ComponentShutdownTriggerHandle,
}

impl CardinalityLimiterProcessor {
  pub async fn new<H: Hasher + Default + Send + 'static>(
    config: &CardinalityLimiterConfig,
    context: ProcessorFactoryContext,
  ) -> anyhow::Result<Self> {
    let limiter = match config.limit_type.as_ref().expect("pgv") {
      Limit_type::GlobalLimit(global_limit) => Box::new(GlobalLimiter::<H>::new(
        global_limit,
        config.buckets as usize,
      )) as Box<dyn Limiter>,
      Limit_type::PerPodLimit(per_pod_limit) => Box::new(K8sPodLimiter::<H>::new(
        per_pod_limit,
        config.buckets as usize,
        (context.k8s_watch_factory)().await?.make_owned(),
        context.shutdown_trigger_handle.make_shutdown(),
      )?),
    };

    Ok(Self {
      dispatcher: context.dispatcher,
      limiter,
      rotate_after: config.rotate_after.unwrap_duration_or(30.minutes()),
      drop: context.scope.counter("drop"),
      shutdown: context.shutdown_trigger_handle,
    })
  }
}

#[async_trait]
impl PipelineProcessor for CardinalityLimiterProcessor {
  async fn recv_samples(self: Arc<Self>, samples: Vec<ParsedMetric>) {
    self
      .dispatcher
      .send(
        samples
          .into_iter()
          .filter_map(|metric| {
            match self
              .limiter
              .limit(metric.metric().get_id(), metric.metadata())
            {
              LimitResult::Ok => Some(metric),
              LimitResult::Drop => {
                self.drop.inc();
                warn_every!(
                  15.seconds(),
                  "dropping new metric '{}' due to cardinality limit",
                  metric.metric().get_id()
                );

                None
              },
            }
          })
          .collect(),
      )
      .await;
  }

  async fn start(self: Arc<Self>) {
    tokio::spawn(async move {
      let mut shutdown = self.shutdown.make_shutdown();

      let mut interval = self.rotate_after.interval_at(MissedTickBehavior::Delay);
      loop {
        tokio::select! {
          _ = interval.tick() => (),
          () = shutdown.cancelled() => break,
        }
        log::debug!("performing cardinality rotation");
        self.limiter.rotate();
      }
    });
  }
}
