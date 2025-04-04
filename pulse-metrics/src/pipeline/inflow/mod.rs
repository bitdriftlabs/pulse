// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use self::metric_generator::MetricGeneratorInflow;
use self::prom_remote_write::PromRemoteWriteInflow;
use self::wire::tcp::TcpInflow;
use self::wire::udp::UdpInflow;
use self::wire::unix::UnixInflow;
use super::PipelineDispatch;
use async_trait::async_trait;
use bd_server_stats::stats::Scope;
use bd_shutdown::ComponentShutdownTriggerHandle;
use pulse_common::bind_resolver::BindResolver;
use pulse_common::k8s::pods_info::K8sWatchFactory;
use pulse_common::singleton::SingletonManager;
use pulse_protobuf::protos::pulse::config::bootstrap::v1::bootstrap::KubernetesBootstrapConfig;
use pulse_protobuf::protos::pulse::config::inflow::v1::inflow::InflowConfig;
use pulse_protobuf::protos::pulse::config::inflow::v1::inflow::inflow_config::Config_type;
use std::sync::Arc;

mod metric_generator;
mod prom_remote_write;
mod prom_scrape;
pub mod wire;

//
// PipelineInflow
//

/// A PipelineInflow is responsible for ingesting data into the [crate::pipeline::MetricPipeline] by
/// using the [crate::pipeline::DispatchSender] to route metric samples to the next hop in the
/// processor.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait PipelineInflow {
  /// Start the inflow (listen on sockets, etc.). This is called after the entire pipeline is
  /// created.
  async fn start(self: Arc<Self>);
}

//
// InflowFactoryContext
//

pub struct InflowFactoryContext {
  pub k8s_config: KubernetesBootstrapConfig,
  pub k8s_watch_factory: K8sWatchFactory,
  pub name: String,
  pub scope: Scope,
  pub dispatcher: Arc<dyn PipelineDispatch>,
  pub shutdown_trigger_handle: ComponentShutdownTriggerHandle,
  pub bind_resolver: Arc<dyn BindResolver>,
  pub singleton_manager: Arc<SingletonManager>,
}

pub(super) type DynamicPipelineInflow = Arc<dyn PipelineInflow + Send + Sync + 'static>;

pub(super) async fn to_inflow(
  config: InflowConfig,
  context: InflowFactoryContext,
) -> anyhow::Result<DynamicPipelineInflow> {
  match config.config_type.expect("pgv") {
    Config_type::Unix(config) => Ok(Arc::new(UnixInflow::new(config, context)?)),
    Config_type::Tcp(config) => Ok(Arc::new(TcpInflow::new(config, context).await?)),
    Config_type::Udp(config) => Ok(Arc::new(UdpInflow::new(config, context).await?)),
    Config_type::PromRemoteWrite(config) => {
      Ok(Arc::new(PromRemoteWriteInflow::new(config, context).await?))
    },
    Config_type::MetricGenerator(config) => {
      Ok(Arc::new(MetricGeneratorInflow::new(config, context)))
    },
    Config_type::K8sProm(config) => prom_scrape::scraper::make(config, context).await,
  }
}
