// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./mod_test.rs"]
mod mod_test;

use self::convert::proto_metric_to_parsed_metric;
use self::shard_map::{ShardMap, shardmap_from_config};
use super::elision::get_last_elided::GetLastElided;
use super::{PipelineProcessor, ProcessorFactoryContext};
use crate::admin::server::Admin;
use crate::clients::retry::Retry;
use crate::pipeline::PipelineDispatch;
use crate::pipeline::processor::internode::shard_map::peer_list_is_match;
use crate::protos::metric::ParsedMetric;
use anyhow::bail;
use async_trait::async_trait;
use axum::Router;
use axum::http::Extensions;
use backoff::ExponentialBackoffBuilder;
use bd_grpc::compression::Compression;
use bd_grpc::service::ServiceMethod;
use bd_grpc::stats::EndpointStats;
use bd_grpc::{Handler, make_unary_router};
use bd_log::warn_every;
use bd_server_stats::stats::{AutoGauge, Scope};
use bd_shutdown::{ComponentShutdown, ComponentShutdownTriggerHandle};
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use http::HeaderMap;
use hyper_util::client::legacy::connect::HttpConnector;
use log::{debug, error, info, trace, warn};
use parking_lot::Mutex;
use prometheus::{Histogram, IntCounter, IntGauge};
use protobuf::Chars;
use pulse_common::bind_resolver::BoundTcpSocket;
use pulse_common::proto::ProtoDurationToStdDuration;
use pulse_common::singleton::SingletonManager;
use pulse_protobuf::protos::pulse::config::processor::v1::internode::InternodeConfig;
use pulse_protobuf::protos::pulse::config::processor::v1::internode::internode_config::NodeConfig;
use pulse_protobuf::protos::pulse::internode::v1::internode::{
  InternodeMetricsRequest,
  InternodeMetricsResponse,
  LastElidedTimestampRequest,
  LastElidedTimestampResponse,
  PeersComparisonRequest,
  PeersComparisonResponse,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use time::Duration;
use time::ext::NumericalDuration;
use tokio::sync::Semaphore;

mod convert;
mod shard_map;

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::milliseconds(100);

//
// Stats
//

#[derive(Clone, Debug)]
struct Stats {
  failthrough_metrics: IntCounter,
  internode_active: IntGauge,
  internode_attempts: IntCounter,
  internode_failure: IntCounter,
  internode_success: IntCounter,
  internode_connect_timeout: IntCounter,
  internode_request_timeout: IntCounter,
  internode_retry: IntCounter,
  internode_overflow: IntCounter,
  internode_time: Histogram,
  internode_convert_failure: IntCounter,
}

impl Stats {
  pub fn new_with_scope(scope: &Scope) -> Self {
    Self {
      failthrough_metrics: scope.counter("failthrough_metrics"),
      internode_active: scope.gauge("internode_active"),
      internode_attempts: scope.counter("internode_attempts"),
      internode_failure: scope.counter("internode_failure"),
      internode_success: scope.counter("internode_success"),
      internode_connect_timeout: scope.counter("internode_connect_timeout"),
      internode_request_timeout: scope.counter("internode_request_timeout"),
      internode_retry: scope.counter("internode_retry"),
      internode_overflow: scope.counter("internode_overflow"),
      internode_time: scope.histogram("internode_time"),
      internode_convert_failure: scope.scope("server").counter("internode_convert_failure"),
    }
  }
}

//
// Client
//

#[derive(Debug)]
struct Client {
  client: bd_grpc::client::Client<HttpConnector>,
  request_timeout: Duration,
}

impl Client {
  fn new(addr: &str, request_timeout: Duration) -> Arc<Self> {
    Arc::new(Self {
      // TODO(mattklein123): Make all of this configurable.
      client: bd_grpc::client::Client::new_http(addr, 100.milliseconds(), 1024).unwrap(),
      request_timeout,
    })
  }
}

//
// InternodeProcessor
//

type OutboundMap = HashMap<usize, (Arc<Client>, Vec<ParsedMetric>)>;

pub struct InternodeProcessor {
  shardmap: ShardMap<Arc<Client>>,
  config: InternodeConfig,
  dispatcher: Arc<dyn PipelineDispatch>,
  shutdown_trigger_handle: ComponentShutdownTriggerHandle,
  stats: Stats,
  endpoint_stats: Arc<EndpointStats>,
  internode_metrics_service_method:
    ServiceMethod<InternodeMetricsRequest, InternodeMetricsResponse>,
  get_peers_comparison_service_method:
    ServiceMethod<PeersComparisonRequest, PeersComparisonResponse>,
  last_elided_timestamp_service_method:
    ServiceMethod<LastElidedTimestampRequest, LastElidedTimestampResponse>,
  singleton_manager: Arc<SingletonManager>,
  admin: Arc<dyn Admin>,
  retry: Arc<Retry>,
  concurrent_requests_semaphore: Arc<Semaphore>,
  socket: Mutex<Option<BoundTcpSocket>>,
  max_pending_requests: u32,
  current_pending_requests: AtomicU32,
}

impl InternodeProcessor {
  pub async fn new(
    config: InternodeConfig,
    context: ProcessorFactoryContext,
  ) -> anyhow::Result<Arc<Self>> {
    let stats = Stats::new_with_scope(&context.scope);
    let retry = Retry::new(&config.request_policy.retry_policy)?;

    let shardmap = match shardmap_from_config(&config, |node_config: &NodeConfig| {
      Client::new(
        &node_config.address,
        config
          .request_policy
          .timeout
          .unwrap_duration_or(DEFAULT_REQUEST_TIMEOUT),
      )
    }) {
      Ok(s) => Ok(s),
      Err(e) => {
        error!("failed to load shardmap due to error {e:?}");
        Err(e)
      },
    }?;

    let max_concurrent_requests = config
      .request_policy
      .get_or_default()
      .max_concurrent_requests
      .unwrap_or_else(|| (4 * shardmap.nodes.len()).try_into().unwrap())
      .try_into()
      .unwrap();
    log::info!("internode max concurrent requests: {max_concurrent_requests}");
    let max_pending_requests = config
      .request_policy
      .get_or_default()
      .max_pending_requests
      .unwrap_or_else(|| u32::try_from(max_concurrent_requests * 2).unwrap());
    log::info!("internode max pending requests: {max_pending_requests}");

    let socket = context.bind_resolver.resolve_tcp(&config.listen).await?;
    info!("starting internode server on {}", socket.local_addr());

    Ok(Arc::new(Self {
      shardmap,
      config,
      dispatcher: context.dispatcher,
      shutdown_trigger_handle: context.shutdown_trigger_handle,
      stats,
      endpoint_stats: Arc::new(EndpointStats::new(context.scope.scope("grpc"))),
      internode_metrics_service_method: ServiceMethod::<
        InternodeMetricsRequest,
        InternodeMetricsResponse,
      >::new("Internode", "InternodeMetrics"),
      get_peers_comparison_service_method: ServiceMethod::<
        PeersComparisonRequest,
        PeersComparisonResponse,
      >::new("Internode", "GetPeersComparison"),
      last_elided_timestamp_service_method: ServiceMethod::<
        LastElidedTimestampRequest,
        LastElidedTimestampResponse,
      >::new("Internode", "LastElidedTimestamp"),
      singleton_manager: context.singleton_manager,
      admin: context.admin,
      retry,
      concurrent_requests_semaphore: Arc::new(Semaphore::new(max_concurrent_requests)),
      socket: Mutex::new(Some(socket)),
      max_pending_requests,
      current_pending_requests: AtomicU32::new(0),
    }))
  }

  fn group_lines_by_shard(&self, lines: Vec<ParsedMetric>) -> (Vec<ParsedMetric>, OutboundMap) {
    let outbound_map = OutboundMap::default();
    lines.into_iter().fold(
      (Vec::<ParsedMetric>::default(), outbound_map),
      |(mut local_lines, mut outbound_map), parsed_metric| {
        if let (index, Some(client)) = self.shardmap.pick_node(parsed_metric.metric().get_id()) {
          trace!("determined client {index:?}");
          let (_, entry) = outbound_map
            .entry(index)
            .or_insert_with(|| (client.clone(), Vec::default()));
          entry.push(parsed_metric);
        } else {
          trace!("determined self as shard");
          local_lines.push(parsed_metric);
        }
        (local_lines, outbound_map)
      },
    )
  }

  async fn make_metrics_request(
    self: Arc<Self>,
    client: Arc<Client>,
    outbound_metrics: Vec<ParsedMetric>,
  ) {
    trace!("sending internode {outbound_metrics:?}");

    if self
      .current_pending_requests
      .fetch_add(1, Ordering::Relaxed)
      >= self.max_pending_requests
    {
      // We have too many pending requests already, so we can't send this one.
      // This can happen if the internode pipeline is overloaded or if there are
      // too many slow nodes in the pipeline.
      self.stats.internode_overflow.inc();
      warn_every!(
        15.seconds(),
        "{}",
        "internode max pending requests exceeded, dropping request"
      );
      self
        .current_pending_requests
        .fetch_sub(1, Ordering::Relaxed);
      return;
    }

    // TODO(mattklein123): The purpose of this sempahore is to allow some amount of progress for
    // the pipeline if there is a slow internode node. This is imperfect as the slow node can
    // still use up all the request slots over time, however this can be no worse then no
    // attempt at all, so seems worth it.
    let permit = self
      .concurrent_requests_semaphore
      .clone()
      .acquire_owned()
      .await
      .unwrap();
    self
      .current_pending_requests
      .fetch_sub(1, Ordering::Relaxed);
    tokio::spawn(async move {
      self.stats.internode_attempts.inc();
      let _timer = self.stats.internode_time.start_timer();
      let _active = AutoGauge::new(self.stats.internode_active.clone());

      let result = self
        .retry
        .retry_notify(
          ExponentialBackoffBuilder::new()
            .with_max_elapsed_time(Some(client.request_timeout.unsigned_abs()))
            .build(),
          || async {
            client
              .client
              .unary(
                &self.internode_metrics_service_method,
                None,
                InternodeMetricsRequest {
                  metrics: outbound_metrics
                    .iter()
                    .map(std::convert::Into::into)
                    .collect(),
                  ..Default::default()
                },
                client.request_timeout,
                Compression::Snappy,
              )
              .await
              .map_err(backoff::Error::transient)
          },
          || {
            self.stats.internode_retry.inc();
          },
        )
        .await;

      match result {
        Ok(_) => {
          // Successful send, do nothing.
          self.stats.internode_success.inc();
        },
        Err(e) => {
          match e {
            bd_grpc::error::Error::ConnectionTimeout => self.stats.internode_connect_timeout.inc(),
            bd_grpc::error::Error::RequestTimeout => self.stats.internode_request_timeout.inc(),
            _ => (),
          }

          warn_every!(
            15.seconds(),
            "internode call error, falling back to local {:?}",
            e
          );
          self.stats.internode_failure.inc();
          self
            .stats
            .failthrough_metrics
            .inc_by(outbound_metrics.len().try_into().unwrap());
          self.dispatcher.send_alt(outbound_metrics).await;
        },
      }

      drop(permit);
    });
  }

  async fn dispatch_outbound_metrics(
    self: &Arc<Self>,
    lines: Vec<ParsedMetric>,
  ) -> Vec<ParsedMetric> {
    let (local_lines, mut outbound_map) = self.group_lines_by_shard(lines);
    if outbound_map.is_empty() {
      return local_lines;
    }

    let mut requests: FuturesUnordered<_> = outbound_map
      .drain()
      .map(|(_, (client, outbound_metrics))| async move {
        self
          .clone()
          .make_metrics_request(client, outbound_metrics)
          .await;
      })
      .collect();

    trace!("requests to dispatch: {}", requests.len());
    while requests.next().await == Some(()) {}

    local_lines
  }

  async fn validate_internode_with_peers(&self) -> bool {
    let mut requests: FuturesUnordered<_> = self
      .shardmap
      .nodes
      .clone()
      .into_iter()
      .filter(|node| !node.is_self)
      .map(|node| {
        (
          node.address.clone(),
          Client::new(
            &node.address,
            self
              .config
              .request_policy
              .timeout
              .unwrap_duration_or(DEFAULT_REQUEST_TIMEOUT),
          ),
        )
      })
      .map(|(addr, client)| async move {
        (
          client
            .client
            .unary(
              &self.get_peers_comparison_service_method,
              None,
              PeersComparisonRequest::default(),
              client.request_timeout,
              Compression::None,
            )
            .await,
          addr,
        )
      })
      .collect();

    let peer_list = self.shardmap.peer_list();
    let mut invalid_matches: u32 = 0;
    while let Some(async_result) = requests.next().await {
      match async_result {
        (Ok(rpc_response), addr) => {
          debug!("Received shardmap from {addr:?}");
          let peers_peers: Vec<Chars> = rpc_response.peer;
          let matches = peer_list_is_match(&peer_list, peers_peers.as_ref(), addr.as_str());
          if matches {
            info!("Shardmap validated with {}", addr.as_str());
          } else {
            invalid_matches += 1;
            warn!(
              "Shardmap differs to {}! Elision results may be unreliable.",
              addr.as_str()
            );
          }
        },
        (Err(e), addr) => {
          warn!("Unable to fetch shardmap from {addr:?}. Got error: {e}");
        },
      }
    }

    invalid_matches == 0
  }

  pub async fn get_last_elided(&self, name: &str) -> anyhow::Result<Option<u64>> {
    let mut requests: FuturesUnordered<_> = self
      .shardmap
      .nodes
      .iter()
      .filter_map(|node| {
        if let Some(client) = &node.inner {
          let request = async move {
            (
              client
                .client
                .unary(
                  &self.last_elided_timestamp_service_method,
                  None,
                  LastElidedTimestampRequest {
                    metric: name.to_string().into(),
                    ..Default::default()
                  },
                  client.request_timeout,
                  Compression::None,
                )
                .await,
              node.address.as_str(),
            )
          };
          return Some(request);
        }
        None
      })
      .collect();

    let mut total_requests = 0;
    let mut valid_responses = 0;
    let mut last_elided = 0;
    let mut sample_err = None;
    while let Some(async_result) = requests.next().await {
      total_requests += 1;
      match async_result {
        (Ok(rpc_response), _) => {
          valid_responses += 1;
          let timestamp = rpc_response.timestamp;
          last_elided = last_elided.max(timestamp);
        },
        (Err(e), addr) => {
          warn!("unable to fetch last_elided from {addr:?}. got error: {e}");
          sample_err = Some(e);
        },
      }
    }

    if let Some(sample_err) = sample_err {
      bail!(
        "internode request error (only received valid responses from {valid_responses} of \
         {total_requests} peers), sample error: {sample_err}"
      );
    }

    Ok(Some(last_elided))
  }
}

#[async_trait]
impl PipelineProcessor for InternodeProcessor {
  async fn recv_samples(self: Arc<Self>, samples: Vec<ParsedMetric>) {
    let local_samples = self.dispatch_outbound_metrics(samples).await;
    if !local_samples.is_empty() {
      self.dispatcher.send(local_samples).await;
    }
  }

  async fn start(self: Arc<Self>) {
    // TODO(mattklein123): There should be 3 phases to startup, instead of 2. Phase 1 is create
    // the entire pipeline and check config. Phase 2 would be for startup such as internode setup,
    // then phase 3 would be for inflows to start accepting connections.
    if !self.validate_internode_with_peers().await {
      warn!("Server configuration does not match a peer's");
    }

    let handler = InternodeHandler::new(
      self.dispatcher.clone(),
      self.stats.clone(),
      self.endpoint_stats.clone(),
      self.shardmap.peer_list(),
      GetLastElided::register_internode(self.clone(), &self.singleton_manager, self.admin.as_ref())
        .await,
    );
    let shutdown = self.shutdown_trigger_handle.make_shutdown();
    let socket = self.socket.lock().take().unwrap();
    tokio::spawn(async move {
      // TODO(mattklein123): If this fails the server should fail to startup.
      if let Err(e) = server(socket, handler, shutdown.clone()).await {
        error!("internode server start failed: {e:?}");
      } else {
        info!("internode server stopped");
      }

      drop(shutdown);
    });
  }
}

pub struct InternodeHandler {
  dispatcher: Arc<dyn PipelineDispatch>,
  stats: Stats,
  endpoint_stats: Arc<EndpointStats>,
  peer_list: Vec<Chars>,
  get_last_elided: Arc<GetLastElided>,
}

impl InternodeHandler {
  fn new(
    dispatcher: Arc<dyn PipelineDispatch>,
    stats: Stats,
    endpoint_stats: Arc<EndpointStats>,
    peer_list: Vec<Chars>,
    get_last_elided: Arc<GetLastElided>,
  ) -> Self {
    Self {
      dispatcher,
      stats,
      endpoint_stats,
      peer_list,
      get_last_elided,
    }
  }
}

#[async_trait]
impl Handler<InternodeMetricsRequest, InternodeMetricsResponse> for InternodeHandler {
  async fn handle(
    &self,
    _headers: HeaderMap,
    _extensions: Extensions,
    request: InternodeMetricsRequest,
  ) -> bd_grpc::error::Result<InternodeMetricsResponse> {
    let metrics: Vec<ParsedMetric> = request
      .metrics
      .into_iter()
      .filter_map(|metric_proto| {
        proto_metric_to_parsed_metric(metric_proto).map_or_else(
          |_| {
            self.stats.internode_convert_failure.inc();
            None
          },
          Some,
        )
      })
      .collect();
    // Internode metrics are all assumed to be correctly sharded, so just
    // send them directly down the pipeline. Re-sharding and sending could
    // result in an infinite loop.
    self.dispatcher.send(metrics).await;
    Ok(InternodeMetricsResponse::default())
  }
}

#[async_trait]
impl Handler<PeersComparisonRequest, PeersComparisonResponse> for InternodeHandler {
  async fn handle(
    &self,
    _headers: HeaderMap,
    _extensions: Extensions,
    _request: PeersComparisonRequest,
  ) -> bd_grpc::error::Result<PeersComparisonResponse> {
    Ok(PeersComparisonResponse {
      peer: self.peer_list.clone(),
      ..Default::default()
    })
  }
}

#[async_trait]
impl Handler<LastElidedTimestampRequest, LastElidedTimestampResponse> for InternodeHandler {
  async fn handle(
    &self,
    _headers: HeaderMap,
    _extensions: Extensions,
    request: LastElidedTimestampRequest,
  ) -> bd_grpc::error::Result<LastElidedTimestampResponse> {
    let name = request.metric;
    match self.get_last_elided.get_last_elided(&name, false).await {
      Ok(last_elided) => Ok(LastElidedTimestampResponse {
        timestamp: last_elided.unwrap_or_default(),
        ..Default::default()
      }),
      Err(e) => Err(bd_grpc::error::Error::Grpc(bd_grpc::status::Status::new(
        bd_grpc_codec::code::Code::Internal,
        e.to_string(),
      ))),
    }
  }
}

fn make_router(handler: &Arc<InternodeHandler>) -> Router {
  make_unary_router(
    &ServiceMethod::<InternodeMetricsRequest, InternodeMetricsResponse>::new(
      "Internode",
      "InternodeMetrics",
    ),
    handler.clone(),
    |_| {},
    Some(&handler.endpoint_stats),
    false,
  )
  .merge(make_unary_router(
    &ServiceMethod::<PeersComparisonRequest, PeersComparisonResponse>::new(
      "Internode",
      "GetPeersComparison",
    ),
    handler.clone(),
    |_| {},
    None,
    false,
  ))
  .merge(make_unary_router(
    &ServiceMethod::<LastElidedTimestampRequest, LastElidedTimestampResponse>::new(
      "Internode",
      "LastElidedTimestamp",
    ),
    handler.clone(),
    |_| {},
    None,
    false,
  ))
}

/// Generate a server for inbound grpc requests
pub async fn server(
  socket: BoundTcpSocket,
  handler: InternodeHandler,
  mut shutdown: ComponentShutdown,
) -> anyhow::Result<()> {
  let server = axum::serve(
    socket.listen(),
    make_router(&Arc::new(handler)).into_make_service(),
  );
  Ok(
    server
      .with_graceful_shutdown(async move {
        shutdown.cancelled().await;
      })
      .await?,
  )
}
