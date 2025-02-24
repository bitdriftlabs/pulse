// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./scraper_test.rs"]
mod scraper_test;

use crate::pipeline::PipelineDispatch;
use crate::pipeline::inflow::prom_scrape::parser::parse_as_metrics;
use crate::pipeline::inflow::{DynamicPipelineInflow, InflowFactoryContext, PipelineInflow};
use crate::pipeline::time::{DurationJitter, RealDurationJitter};
use crate::protos::metric::{
  DownstreamId,
  Metric,
  MetricId,
  MetricSource,
  MetricType,
  MetricValue,
  ParsedMetric,
  TagValue,
  default_timestamp,
};
use async_trait::async_trait;
use bd_log::warn_every;
use bd_server_stats::stats::Scope;
use bd_shutdown::{ComponentShutdown, ComponentShutdownTrigger, ComponentShutdownTriggerHandle};
use bd_time::{ProtoDurationExt, TimeDurationExt};
use futures_util::future::{join_all, pending};
use http::StatusCode;
use http::header::{ACCEPT, AUTHORIZATION};
use itertools::Itertools;
use k8s_prom::KubernetesPrometheusConfig;
use k8s_prom::kubernetes_prometheus_config::pod::InclusionFilter;
use k8s_prom::kubernetes_prometheus_config::pod::inclusion_filter::Filter_type;
use k8s_prom::kubernetes_prometheus_config::{self, Target};
use parking_lot::Mutex;
use prometheus::IntCounter;
use pulse_common::k8s::pods_info::{OwnedPodsInfoSingleton, PodInfo};
use pulse_common::k8s::{NodeInfo, missing_node_name_error};
use pulse_common::metadata::Metadata;
use pulse_common::proto::env_or_inline_to_string;
use pulse_protobuf::protos::pulse::config::bootstrap::v1::bootstrap::KubernetesBootstrapConfig;
use pulse_protobuf::protos::pulse::config::inflow::v1::k8s_prom;
use regex::Regex;
use std::collections::{BTreeMap, HashMap};
use std::iter::empty;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Instant;
use time::Duration;
use time::ext::NumericalDuration;
use tokio::time::MissedTickBehavior;

fn process_inclusion_filters(
  inclusion_filters: &[InclusionFilter],
  pod_info: &PodInfo,
) -> Vec<i32> {
  inclusion_filters
    .iter()
    .flat_map(
      |inclusion_filter| match inclusion_filter.filter_type.as_ref().expect("pgv") {
        Filter_type::ContainerPortNameRegex(regex) => Regex::new(regex).ok().map_or_else(
          || empty().collect(),
          |regex| {
            pod_info
              .container_ports
              .iter()
              .filter_map(|port| {
                if regex.is_match(&port.name) {
                  Some(port.port)
                } else {
                  None
                }
              })
              .collect_vec()
          },
        ),
      },
    )
    .collect()
}

fn create_endpoints(
  inclusion_filters: &[InclusionFilter],
  pod_info: &PodInfo,
  service_name: Option<&str>,
  maybe_service_port: Option<i32>,
  prom_annotations: &BTreeMap<String, String>,
) -> Vec<(String, PromEndpoint)> {
  let included_ports = process_inclusion_filters(inclusion_filters, pod_info);
  if prom_annotations
    .get("prometheus.io/scrape")
    .map(String::as_str)
    != Some("true")
    && included_ports.is_empty()
  {
    return Vec::new();
  }

  let prom_namespace = prom_annotations
    .get("prometheus.io/namespace")
    .cloned()
    .unwrap_or_else(|| pod_info.namespace.to_string());

  let prom_endpoint_path = prom_annotations
    .get("prometheus.io/path")
    .cloned()
    .unwrap_or_else(|| "/metrics".to_string());

  // We attempt the resolve the prom endpoint by considering (in order):
  // 1. A service annotation that specifies valid port number(s) via prometheus.io/port
  // 2. A a service port on the service. We use the first port mapping present on the svc object.
  // 3. A default of port 9090 if 1 & 2 are not present.
  let ports: Vec<i32> = prom_annotations
    .get("prometheus.io/port")
    .into_iter()
    .flat_map(|port| port.split(',').filter_map(|p| p.parse().ok()))
    .chain(included_ports)
    .unique()
    .collect_vec();

  let ports = match (maybe_service_port, ports.is_empty()) {
    (_, false) => ports,
    (Some(service_port), true) => vec![service_port],
    (None, true) => vec![9090],
  };

  ports
    .iter()
    .map(|port| {
      (
        format!(
          "{}/{}/{}/{}",
          prom_namespace,
          service_name.unwrap_or_default(),
          pod_info.name,
          port
        ),
        PromEndpoint::new(
          "http",
          pod_info.ip.to_string(),
          *port,
          &prom_endpoint_path,
          Some(Arc::new(Metadata::new(
            &prom_namespace,
            &pod_info.name,
            &pod_info.ip.to_string(),
            &pod_info.labels,
            &pod_info.annotations,
            service_name,
            &pod_info.node_name,
            &pod_info.node_ip,
            Some(format!("{}:{port}", pod_info.ip)),
          ))),
        ),
      )
    })
    .collect()
}

//
// PromEndpoint
//

/// When scraping prometheus metrics, we think of an endpoint as a single source to scrape metrics
/// from with attached metadata that may be used to enrich or modify the scraped metrics.
///
/// In k8s parlance each endpoint maps to a single pod, augmented by service annotations that came
/// from the service that spawned the endpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PromEndpoint {
  address: String,
  port: i32,
  url: String,
  metadata: Option<Arc<Metadata>>,
}

impl PromEndpoint {
  fn new(
    scheme: &str,
    address: String,
    port: i32,
    path: &str,
    metadata: Option<Arc<Metadata>>,
  ) -> Self {
    let url = format!("{scheme}://{address}:{port}{path}");
    Self {
      address,
      port,
      url,
      metadata,
    }
  }

  const fn metadata(&self) -> Option<&Arc<Metadata>> {
    self.metadata.as_ref()
  }

  async fn scrape(
    &self,
    client: &reqwest::Client,
    k8s_service_account: bool,
  ) -> anyhow::Result<(String, StatusCode)> {
    let mut request = client.get(&self.url).header(ACCEPT, "text/plain");

    if k8s_service_account {
      request = request.header(
        AUTHORIZATION,
        format!(
          "Bearer {}",
          std::str::from_utf8(&std::fs::read(
            "/var/run/secrets/kubernetes.io/serviceaccount/token"
          )?)?
        ),
      );
    }
    let response = request.send().await?;

    let status = response.status();

    Ok((response.text().await?, status))
  }
}

//
// Stats
//

#[derive(Clone)]
pub struct Stats {
  scrape_attempt: IntCounter,
  scrape_failure: IntCounter,
  scrape_complete: IntCounter,
  parse_failure: IntCounter,
}

impl Stats {
  fn new(scope: &Scope) -> Self {
    Self {
      scrape_attempt: scope.counter("scrape_attempt"),
      scrape_failure: scope.counter("scrape_failure"),
      scrape_complete: scope.counter("scrape_complete"),
      parse_failure: scope.counter("parse_failure"),
    }
  }
}

//
// Scraper
//

/// Monitors Prometheus endpoints and collects metrics from each active endpoint. The main loop
/// awaits changes to the watched pod set, which then spawns tasks that are responsible for polling
/// the relevant endpoints at the correct frequency.
struct Scraper<Provider: EndpointProvider, Jitter: DurationJitter> {
  name: String,
  stats: Stats,
  dispatcher: Arc<dyn PipelineDispatch>,
  shutdown_trigger_handle: ComponentShutdownTriggerHandle,
  endpoints: Mutex<Option<Provider>>,
  k8s_service_account: bool,
  scrape_interval: Duration,
  client: reqwest::Client,
  ticker_factory: Box<dyn Fn() -> Box<dyn Ticker> + Send + Sync>,
  jitter: PhantomData<Jitter>,
  emit_up_metric: bool,
}

impl<Provider: EndpointProvider + 'static, Jitter: DurationJitter + 'static>
  Scraper<Provider, Jitter>
{
  fn create(
    name: String,
    stats: Stats,
    dispatcher: Arc<dyn PipelineDispatch>,
    shutdown_trigger_handle: ComponentShutdownTriggerHandle,
    endpoints: Provider,
    k8s_service_account_caller: bool,
    scrape_interval: Duration,
    ticker_factory: Box<dyn Fn() -> Box<dyn Ticker> + Send + Sync>,
    emit_up_metric: bool,
  ) -> anyhow::Result<DynamicPipelineInflow> {
    let client = if k8s_service_account_caller {
      reqwest::Client::builder()
        .add_root_certificate(reqwest::Certificate::from_pem(&std::fs::read(
          "/var/run/secrets/kubernetes.io/serviceaccount/ca.crt",
        )?)?)
        .danger_accept_invalid_certs(true)
        .build()?
    } else {
      reqwest::Client::new()
    };

    Ok(Arc::new(Self {
      name,
      stats,
      dispatcher,
      client,
      endpoints: Mutex::new(Some(endpoints)),
      k8s_service_account: k8s_service_account_caller,
      scrape_interval,
      shutdown_trigger_handle,
      ticker_factory,
      jitter: PhantomData,
      emit_up_metric,
    }))
  }

  /// Periodically scrapes a single endpoint for Prometheus metrics.
  async fn scrape_endpoint(
    self: &Arc<Self>,
    mut shutdown: ComponentShutdown,
    id: String,
    prom_endpoint: PromEndpoint,
  ) {
    let initial_jitter = Jitter::full_jitter_duration(self.scrape_interval);
    log::debug!(
      "starting scrape job {id} with {} seconds of jitter",
      initial_jitter.whole_seconds()
    );

    // Make sure we respect the cancellation if it happens during jitter.
    tokio::select! {
        () = initial_jitter.sleep() => {},
        () = shutdown.cancelled() => {
          log::debug!("scrape job cancelled while waiting for initial jitter");
          return;
        }
    }

    self
      .scrape_endpoint_inner(shutdown, id, prom_endpoint)
      .await;
  }

  async fn scrape_endpoint_inner(
    self: &Arc<Self>,
    mut shutdown: ComponentShutdown,
    id: String,
    prom_endpoint: PromEndpoint,
  ) {
    let mut ticker = (self.ticker_factory)();
    loop {
      tokio::select! {
        () = shutdown.cancelled() => {
          log::debug!("scrape endpoint canceled");
          return
        },
        () = ticker.next() => {}
      }

      log::debug!("performing scrape");
      self.stats.scrape_attempt.inc();
      let lines = match prom_endpoint
        .scrape(&self.client, self.k8s_service_account)
        .await
      {
        Ok((lines, status)) => {
          if status == 200 {
            Some(lines)
          } else {
            warn_every!(
              1.minutes(),
              "failed to scrape prometheus endpoint {}, got {} code",
              id,
              status
            );
            self.stats.scrape_failure.inc();
            None
          }
        },
        Err(e) => {
          warn_every!(
            1.minutes(),
            "failed to scrape prometheus endpoint {}: {}",
            id,
            e
          );
          self.stats.scrape_failure.inc();
          None
        },
      };

      let timestamp = default_timestamp();
      let now = Instant::now();
      let parsed_metrics = lines.and_then(|lines| {
        match parse_as_metrics(&lines, timestamp, now, prom_endpoint.metadata()) {
          Ok(metrics) => {
            self.stats.scrape_complete.inc();
            Some(metrics)
          },
          Err(e) => {
            warn_every!(
              1.minutes(),
              "failed to parse prom response from {}: {}",
              id,
              e
            );
            self.stats.parse_failure.inc();
            None
          },
        }
      });

      let success = parsed_metrics.is_some();
      let mut parsed_metrics = parsed_metrics.unwrap_or_default();
      if self.emit_up_metric {
        let mut metric = ParsedMetric::new(
          Metric::new(
            MetricId::new(
              "up".into(),
              Some(MetricType::Gauge),
              vec![TagValue {
                tag: "instance".into(),
                value: format!("{}:{}", prom_endpoint.address, prom_endpoint.port).into(),
              }],
              false,
            )
            .unwrap(),
            None,
            timestamp,
            if success {
              MetricValue::Simple(1.0)
            } else {
              MetricValue::Simple(0.0)
            },
          ),
          MetricSource::PromRemoteWrite,
          now,
          DownstreamId::LocalOrigin,
        );
        metric.set_metadata(prom_endpoint.metadata().cloned());
        parsed_metrics.push(metric);
      }

      if !parsed_metrics.is_empty() {
        self.dispatcher.send(parsed_metrics).await;
      }
    }
  }

  async fn reload(
    self: &Arc<Self>,
    active_jobs: &mut HashMap<String, ComponentShutdownTrigger>,
    endpoints: &mut Provider,
  ) {
    let mut added = Vec::new();
    let updated_state = {
      // TODO(snowp): Right now we make two passes over all the endpoint as they are first created
      // by get() then we traverse again to consolidate the jobs. Fix this.
      let current_endpoints = endpoints.get();
      let mut updated_state = HashMap::new();
      for (id, endpoint) in &current_endpoints {
        if let Some(job) = active_jobs.remove(id) {
          updated_state.insert(id.to_string(), job);
        } else {
          let shutdown_trigger = ComponentShutdownTrigger::default();
          let shutdown = shutdown_trigger.make_shutdown();
          let cloned_self = self.clone();
          let endpoint = endpoint.clone();
          added.push(id.clone());
          let cloned_id = id.clone();
          tokio::spawn(async move {
            cloned_self
              .scrape_endpoint(shutdown, cloned_id, endpoint)
              .await;
          });
          updated_state.insert(id.clone(), shutdown_trigger);
        }
      }

      updated_state
    };

    log::debug!(
      "({}) updating prom endpoints to: {:?}",
      self.name,
      updated_state.keys()
    );

    if !added.is_empty() {
      log::info!("({}) adding: {:?}", self.name, added);
    }
    if !active_jobs.is_empty() {
      log::info!("({}) removing: {:?}", self.name, active_jobs.keys());
    }

    // All remaining endpoints are no longer referenced and should therefore be completed.
    join_all(
      std::mem::replace(active_jobs, updated_state)
        .into_values()
        .map(bd_shutdown::ComponentShutdownTrigger::shutdown),
    )
    .await;

    log::debug!("({}) completed reload", self.name);
  }
}

#[async_trait]
impl<Provider: EndpointProvider + 'static, Jitter: DurationJitter + 'static> PipelineInflow
  for Scraper<Provider, Jitter>
{
  async fn start(self: Arc<Self>) {
    log::info!("starting k8s prometheus scraper");

    let mut active_jobs = HashMap::new();
    let mut endpoints = self.endpoints.lock().take().unwrap();

    self.reload(&mut active_jobs, &mut endpoints).await;

    tokio::spawn(async move {
      let mut shutdown = self.shutdown_trigger_handle.make_shutdown();
      let shutdown = shutdown.cancelled();
      tokio::pin!(shutdown);
      loop {
        tokio::select! {
           () = endpoints.changed() => {
             self.reload(&mut active_jobs, &mut endpoints).await;
           }
           () = &mut shutdown => {
            log::info!("prometheus scraper cancelled");
            join_all(
              active_jobs.into_values().map(|shutdown_trigger| {
                  shutdown_trigger.shutdown()
                }),
            )
            .await;
            break;
           }
        }
      }
    });
  }
}

// We use our own ticker implementation to control when scraping occurs in test.
#[async_trait]
trait Ticker: Send + Sync {
  async fn next(&mut self);
}

// We use a interval over a sleep to better align with the intended interval to avoid a slow
// upstream from impacting how often we collect.
#[async_trait]
impl Ticker for tokio::time::Interval {
  async fn next(&mut self) {
    self.tick().await;
  }
}

pub async fn make(
  config: KubernetesPrometheusConfig,
  context: InflowFactoryContext,
) -> anyhow::Result<DynamicPipelineInflow> {
  let scrape_interval = config
    .scrape_interval
    .as_ref()
    .expect("pgv")
    .to_time_duration();

  let stats = Stats::new(&context.scope);
  let ticker_factory = Box::new(move || {
    Box::new(scrape_interval.interval(MissedTickBehavior::Delay)) as Box<dyn Ticker>
  });
  match config.target.expect("pgv") {
    Target::Pod(pod_config) => Scraper::<_, RealDurationJitter>::create(
      context.name,
      stats,
      context.dispatcher,
      context.shutdown_trigger_handle,
      KubePodTarget {
        inclusion_filters: pod_config.inclusion_filters,
        pods_info: (context.k8s_watch_factory)().await?.make_owned(),
      },
      false,
      scrape_interval,
      ticker_factory,
      config.emit_up_metric,
    ),
    Target::Endpoint(_) => Scraper::<_, RealDurationJitter>::create(
      context.name,
      stats,
      context.dispatcher,
      context.shutdown_trigger_handle,
      KubeEndpointsTarget {
        pods_info: (context.k8s_watch_factory)().await?.make_owned(),
      },
      false,
      scrape_interval,
      ticker_factory,
      config.emit_up_metric,
    ),
    Target::Node(details) => Scraper::<_, RealDurationJitter>::create(
      context.name,
      stats,
      context.dispatcher,
      context.shutdown_trigger_handle,
      NodeEndpointsTarget::new(context.k8s_config, details).await?,
      true,
      scrape_interval,
      ticker_factory,
      config.emit_up_metric,
    ),
  }
}

// TODO(snowp): At the moment the scraper and the pod watcher is oddly coupled together, see if we
// can make this better. For example, if we want to match what prom can do we'd want to have the
// pod cache store all annotations have the per scraper configuration decide which annotations
// should be used to construct the url.

/// Abstraction around a source of endpoints that can be scraped.
#[async_trait]
trait EndpointProvider: Send + Sync {
  /// Retrieves the current set of endpoints from this provider, keyed by some unique id.
  fn get(&mut self) -> HashMap<String, PromEndpoint>;

  /// Returns a future that resolves once the underlying endpoints have changed.
  async fn changed(&mut self);
}

//
// KubePodTarget
//

/// Resolves prom endpoints via node-local Kubernetes pods.
struct KubePodTarget {
  inclusion_filters: Vec<InclusionFilter>,
  pods_info: OwnedPodsInfoSingleton,
}

#[async_trait]
impl EndpointProvider for KubePodTarget {
  fn get(&mut self) -> HashMap<String, PromEndpoint> {
    self
      .pods_info
      .borrow_and_update()
      .pods()
      .flat_map(|(_, pod_info)| {
        create_endpoints(
          &self.inclusion_filters,
          pod_info,
          None,
          None,
          &pod_info.annotations,
        )
      })
      .collect::<HashMap<String, PromEndpoint>>()
  }

  async fn changed(&mut self) {
    let _ignored = self.pods_info.changed().await;
  }
}

//
// KubeEndpointsTarget
//

/// Resolves prom endpoints via node-local Kubernetes endpoints.
struct KubeEndpointsTarget {
  pods_info: OwnedPodsInfoSingleton,
}

#[async_trait]
impl EndpointProvider for KubeEndpointsTarget {
  fn get(&mut self) -> HashMap<String, PromEndpoint> {
    let mut endpoints = HashMap::new();
    let pods = self.pods_info.borrow_and_update();
    for (_, pod_info) in pods.pods() {
      for service in pod_info.services.values() {
        endpoints.extend(create_endpoints(
          &[],
          pod_info,
          Some(&service.name),
          service.maybe_service_port,
          &service.annotations,
        ));
      }
    }

    endpoints
  }

  async fn changed(&mut self) {
    let _ignored = self.pods_info.changed().await;
  }
}

//
// NodeEndpointsTarget
//

/// Resolve a single prom endpoint which hits the local kubelet port.
struct NodeEndpointsTarget {
  endpoints: HashMap<String, PromEndpoint>,
}

impl NodeEndpointsTarget {
  async fn new(
    kubernetes: KubernetesBootstrapConfig,
    details: kubernetes_prometheus_config::Node,
  ) -> anyhow::Result<Self> {
    let node_name = env_or_inline_to_string(
      &kubernetes
        .node_name
        .into_option()
        .ok_or_else(missing_node_name_error)?,
    )
    .ok_or_else(missing_node_name_error)?;
    let node_info = NodeInfo::new(&node_name).await;

    Ok(Self {
      endpoints: HashMap::from([(
        node_name.to_string(),
        // TODO(mattklein123): Potentially merge in node level metadata?
        PromEndpoint::new(
          "https",
          node_name,
          node_info.kubelet_port,
          &details.path,
          None,
        ),
      )]),
    })
  }
}

#[async_trait]
impl EndpointProvider for NodeEndpointsTarget {
  fn get(&mut self) -> HashMap<String, PromEndpoint> {
    self.endpoints.clone()
  }

  async fn changed(&mut self) {
    pending::<()>().await;
  }
}
