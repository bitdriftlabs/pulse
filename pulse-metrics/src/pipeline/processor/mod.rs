// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use self::aggregation::AggregationProcessor;
use self::buffer::BufferProcessor;
use self::cardinality_limiter::CardinalityLimiterProcessor;
use self::elision::ElisionProcessor;
use self::internode::InternodeProcessor;
use self::mutate::MutateProcessor;
use self::populate_cache::PopulateCacheProcessor;
use self::regex::RegexProcessor;
use self::sampler::SamplerProcessor;
use super::PipelineDispatch;
use super::metric_cache::MetricCache;
use super::time::{RealDurationJitter, TimeProvider};
use crate::admin::server::Admin;
use crate::protos::metric::ParsedMetric;
use ahash::AHasher;
use async_trait::async_trait;
use bd_server_stats::stats::Scope;
use bd_shutdown::ComponentShutdownTriggerHandle;
use cardinality_tracker::CardinalityTrackerProcessor;
use drop::DropProcessor;
use processor::ProcessorConfig;
use processor::processor_config::Processor_type;
use pulse_common::bind_resolver::BindResolver;
use pulse_common::k8s::pods_info::K8sWatchFactory;
use pulse_common::singleton::SingletonManager;
use pulse_protobuf::protos::pulse::config::processor::v1::processor;
use std::sync::Arc;

mod aggregation;
mod buffer;
mod cardinality_limiter;
mod cardinality_tracker;
mod drop;
pub mod elision;
mod internode;
mod mutate;
mod populate_cache;
mod regex;
pub mod sampler;

//
// PipelineProcessor
//

#[async_trait]
#[cfg_attr(test, mockall::automock)]
pub trait PipelineProcessor {
  /// Receive samples from the [`crate::pipeline::MetricsPipeline`] dispatch loop.
  /// Processing occurs asynchronously and results can be sent back to the dispatch
  /// loop through the [`crate::pipeline::DispatchSender`] channel.
  async fn recv_samples(self: Arc<Self>, samples: Vec<ParsedMetric>);

  /// Start the inflow (listen on sockets, etc.). This is called after the entire pipeline is
  /// created.
  async fn start(self: Arc<Self>);
}

pub type DynamicPipelineProcessor = Arc<dyn PipelineProcessor + Send + Sync + 'static>;

//
// ProcessorFactoryContext
//

pub struct ProcessorFactoryContext {
  pub k8s_watch_factory: K8sWatchFactory,
  pub name: String,
  pub scope: Scope,
  pub metric_cache: Arc<MetricCache>,
  pub dispatcher: Arc<dyn PipelineDispatch>,
  pub shutdown_trigger_handle: ComponentShutdownTriggerHandle,
  pub singleton_manager: Arc<SingletonManager>,
  pub admin: Arc<dyn Admin>,
  pub bind_resolver: Arc<dyn BindResolver>,
  pub time_provider: Box<dyn TimeProvider>,
}

pub(super) async fn to_processor(
  config: ProcessorConfig,
  context: ProcessorFactoryContext,
) -> anyhow::Result<DynamicPipelineProcessor> {
  match config.processor_type.expect("pgv") {
    Processor_type::Elision(config) => Ok(ElisionProcessor::new(config, context).await?),
    Processor_type::Internode(config) => Ok(InternodeProcessor::new(config, context).await?),
    Processor_type::Sampler(config) => Ok(Arc::new(SamplerProcessor::<RealDurationJitter>::new(
      config, context,
    ))),
    Processor_type::PopulateCache(_) => Ok(Arc::new(PopulateCacheProcessor::new(context))),
    Processor_type::Aggregation(config) => {
      Ok(AggregationProcessor::new::<RealDurationJitter>(config, context).await?)
    },
    Processor_type::Buffer(config) => Ok(BufferProcessor::new(&config, context)),
    Processor_type::Mutate(config) => Ok(Arc::new(MutateProcessor::new(&config, context)?)),
    Processor_type::CardinalityLimiter(config) => Ok(Arc::new(
      CardinalityLimiterProcessor::new::<AHasher>(&config, context).await?,
    )),
    Processor_type::CardinalityTracker(config) => {
      Ok(CardinalityTrackerProcessor::new(config, context)?)
    },
    Processor_type::Regex(config) => Ok(Arc::new(RegexProcessor::new(&config, context)?)),
    Processor_type::Drop(config) => Ok(Arc::new(DropProcessor::new(config, context).await?)),
  }
}
