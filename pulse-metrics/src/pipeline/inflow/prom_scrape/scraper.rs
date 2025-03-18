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
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use k8s_prom::KubernetesPrometheusConfig;
use k8s_prom::kubernetes_prometheus_config::pod::inclusion_filter::Filter_type;
use k8s_prom::kubernetes_prometheus_config::pod::use_k8s_https_service_auth_matcher::Auth_matcher;
use k8s_prom::kubernetes_prometheus_config::pod::{InclusionFilter, UseK8sHttpsServiceAuthMatcher};
use k8s_prom::kubernetes_prometheus_config::{self, TLS, Target};
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
use std::hash::{Hash, Hasher};
use std::iter::empty;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Instant;
use time::Duration;
use time::ext::NumericalDuration;
use tokio::time::MissedTickBehavior;
use xxhash_rust::xxh64::Xxh64Builder;

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

fn process_k8s_https_service_auth_matchers(
  use_k8s_https_service_auth_matchers: &[UseK8sHttpsServiceAuthMatcher],
  pod_info: &PodInfo,
) -> bool {
  use_k8s_https_service_auth_matchers.iter().any(|matcher| {
    match matcher.auth_matcher.as_ref().expect("pgv") {
      Auth_matcher::AnnotationMatcher(matcher) => pod_info
        .annotations
        .get(matcher.key.as_str())
        .is_some_and(|value| {
          matcher
            .value
            .as_ref()
            .is_none_or(|expected_value| value.as_str() == expected_value.as_str())
        }),
    }
  })
}

/// Resolves the ports to use for scraping metrics from a pod.
/// 
/// The resolution follows this priority order:
/// 1. Use ports from annotations/inclusion filters if available
/// 2. Use service port if specified (either as number or by matching port name)
/// 3. Fall back to default port 9090 if no other ports are available
pub fn resolve_ports(
  ports: Vec<i32>,
  maybe_service_port: Option<&IntOrString>,
  pod_info: &PodInfo,
) -> Vec<i32> {
  // If we have ports from annotations/inclusion filters, use those
  if !ports.is_empty() {
    return ports;
  }

  // Try to resolve from service port
  let service_port = match maybe_service_port {
    Some(IntOrString::Int(port)) => Some(vec![*port]),
    Some(IntOrString::String(name)) => {
      pod_info
        .container_ports
        .iter()
        .find(|port| port.name == *name)
        .map(|port| vec![port.port])
    }
    None => None,
  };

  // If we found a service port, use it, otherwise fall back to default
  service_port.unwrap_or_else(|| vec![9090])
}

fn create_endpoints(
  inclusion_filters: &[InclusionFilter],
  use_k8s_https_service_auth_matchers: &[UseK8sHttpsServiceAuthMatcher],
  pod_info: &PodInfo,
  service_name: Option<&str>,
  maybe_service_port: Option<&IntOrString>,
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
    .map_or_else(
      || "/metrics".to_string(),
      |path| {
        if path.starts_with('/') {
          path
        } else {
          format!("/{path}")
        }
      },
    );

  let ports: Vec<i32> = prom_annotations
    .get("prometheus.io/port")
    .into_iter()
    .flat_map(|port| {
      port
        .split(',')
        .map(str::trim)
        .filter_map(|p| p.parse().ok())
    })
    .chain(included_ports)
    .unique()
    .collect_vec();

  let ports = resolve_ports(ports, maybe_service_port, pod_info);

  let use_k8s_https_service_auth =
    process_k8s_https_service_auth_matchers(use_k8s_https_service_auth_matchers, pod_info);

  ports
    .iter()
    .map(|port| {
      let endpoint = PromEndpoint::new(
        pod_info.ip.to_string(),
        *port,
        prom_endpoint_path.clone(),
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
        use_k8s_https_service_auth,
      );
      // We use a stable hash to make sure the ID changes when the endpoint changes.
      let mut hasher = Xxh64Builder::new(0).build();
      endpoint.hash(&mut hasher);
      let hash = hasher.finish();

      (
        format!(
          "{}/{}/{}/{}/{}",
          prom_namespace,
          service_name.unwrap_or_default(),
          pod_info.name,
          port,
          hash
        ),
        endpoint,
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
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct PromEndpoint {
  address: String,
  port: i32,
  path: String,
  metadata: Option<Arc<Metadata>>,
  use_https_k8s_service_auth: bool,
}

impl PromEndpoint {
  const fn new(
    address: String,
    port: i32,
    path: String,
    metadata: Option<Arc<Metadata>>,
    use_https_k8s_service_auth: bool,
  ) -> Self {
    Self {
      address,
      port,
      path,
      metadata,
      use_https_k8s_service_auth,
    }
  }

  const fn metadata(&self) -> Option<&Arc<Metadata>> {
    self.metadata.as_ref()
  }

  async fn scrape(&self, client: &reqwest::Client) -> anyhow::Result<(String, StatusCode)> {
    let mut request = client
      .get(format!(
        "{}://{}:{}{}",
        if self.use_https_k8s_service_auth {
          "https"
        } else {
          "http"
        },
        self.address,
        self.port,
        self.path
      ))
      .header(ACCEPT, "text/plain");

    if self.use_https_k8s_service_auth {
      // TODO(mattklein123): Read this once on startup.
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
  scrape_interval: Duration,
  http_client: reqwest::Client,
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
    scrape_interval: Duration,
    ticker_factory: Box<dyn Fn() -> Box<dyn Ticker> + Send + Sync>,
    emit_up_metric: bool,
    tls_config: Option<&TLS>,
  ) -> anyhow::Result<DynamicPipelineInflow> {
    fn make_https_client(tls_config: Option<&TLS>) -> anyhow::Result<reqwest::Client> {
      let mut builder =
        reqwest::Client::builder().add_root_certificate(reqwest::Certificate::from_pem(
          &std::fs::read("/var/run/secrets/kubernetes.io/serviceaccount/ca.crt")?,
        )?);

      if let Some(tls) = tls_config {
        if let (Some(cert_file), Some(key_file)) = (tls.cert_file.as_ref(), tls.key_file.as_ref()) {
          let cert = std::fs::read(cert_file)?;
          let key = std::fs::read(key_file)?;
          builder = builder.identity(reqwest::Identity::from_pem(&[cert, key].concat())?);
        }
      }

      // TODO(mattklein123): This was here for a while, and then I removed it thinking that it
      // shouldn't be needed, but connections to K8s APIs still fail without it. We should
      // investigate why this is actually required since AFAICT the public cert above should
      // allow for validation of the server cert.
      Ok(
        builder
          .danger_accept_invalid_certs(tls_config.is_some_and(|tls| tls.insecure_skip_verify))
          .build()?,
      )
    }

    // In practice we should always have a valid CA cert, but this won't work in tests, so we
    // fall back to a basic client if we can't find it.
    let http_client = make_https_client(tls_config)
      .inspect_err(|e| {
        log::warn!("could not create K8s service account HTTPS client, falling back to basic: {e}");
      })
      .or_else(|_| Ok::<_, anyhow::Error>(reqwest::Client::new()))?;

    Ok(Arc::new(Self {
      name,
      stats,
      dispatcher,
      http_client,
      endpoints: Mutex::new(Some(endpoints)),
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
      let lines = match prom_endpoint.scrape(&self.http_client).await {
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
            "failed to scrape prometheus endpoint {}: {:#}",
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
  let tls_config = config.tls_config.into_option();
  match config.target.expect("pgv") {
    Target::Pod(pod_config) => Scraper::<_, RealDurationJitter>::create(
      context.name,
      stats,
      context.dispatcher,
      context.shutdown_trigger_handle,
      KubePodTarget {
        inclusion_filters: pod_config.inclusion_filters,
        use_k8s_https_service_auth_matchers: pod_config.use_k8s_https_service_auth_matchers,
        pods_info: (context.k8s_watch_factory)().await?.make_owned(),
      },
      scrape_interval,
      ticker_factory,
      config.emit_up_metric,
      tls_config.as_ref(),
    ),
    Target::Endpoint(_) => Scraper::<_, RealDurationJitter>::create(
      context.name,
      stats,
      context.dispatcher,
      context.shutdown_trigger_handle,
      KubeEndpointsTarget {
        pods_info: (context.k8s_watch_factory)().await?.make_owned(),
      },
      scrape_interval,
      ticker_factory,
      config.emit_up_metric,
      tls_config.as_ref(),
    ),
    Target::Node(details) => Scraper::<_, RealDurationJitter>::create(
      context.name,
      stats,
      context.dispatcher,
      context.shutdown_trigger_handle,
      NodeEndpointsTarget::new(context.k8s_config, details).await?,
      scrape_interval,
      ticker_factory,
      config.emit_up_metric,
      tls_config.as_ref(),
    ),
  }
}

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
  use_k8s_https_service_auth_matchers: Vec<UseK8sHttpsServiceAuthMatcher>,
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
          &self.use_k8s_https_service_auth_matchers,
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
          &[],
          pod_info,
          Some(&service.name),
          service.maybe_service_port.as_ref(),
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
          node_name,
          node_info.kubelet_port,
          details.path.to_string(),
          None,
          true,
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
