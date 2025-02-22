// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::admin::test::MockAdmin;
use crate::pipeline::MockPipelineDispatch;
use crate::pipeline::metric_cache::MetricCache;
use crate::pipeline::processor::ProcessorFactoryContext;
use crate::pipeline::time::TestTimeProvider;
use crate::protos::metric::{
  CounterType,
  DownstreamId,
  DownstreamIdProvider,
  Metric,
  MetricId,
  MetricSource,
  MetricType,
  MetricValue,
  ParsedMetric,
  TagValue,
};
use bd_server_stats::stats::Collector;
use bd_server_stats::test::util::stats::Helper as StatsHelper;
use bd_shutdown::ComponentShutdownTrigger;
use futures_util::FutureExt;
use pulse_common::bind_resolver::MockBindResolver;
use pulse_common::k8s::pods_info::PodsInfoSingleton;
use pulse_common::k8s::pods_info::container::PodsInfo;
use pulse_common::metadata::Metadata;
use pulse_common::singleton::SingletonManager;
use pulse_protobuf::protos::pulse::config::common::v1::common::WireProtocol;
use pulse_protobuf::protos::pulse::config::common::v1::common::wire_protocol::{
  Carbon,
  Protocol_type,
};
use std::fs;
use std::io::Write;
use std::os::unix::fs as os_fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tempfile::TempDir;
use tokio::sync::watch;

#[cfg(test)]
pub mod thread_synchronizer;

pub struct TestDownstreamIdProvider {}

impl DownstreamIdProvider for TestDownstreamIdProvider {
  fn downstream_id(&self, _metric_id: &MetricId) -> DownstreamId {
    DownstreamId::LocalOrigin
  }
}

#[must_use]
pub fn make_tag(tag: &'static str, value: &'static str) -> TagValue {
  TagValue {
    tag: tag.into(),
    value: value.into(),
  }
}

#[must_use]
pub fn make_metric_id(name: &str, mtype: Option<MetricType>, tags: &[(&str, &str)]) -> MetricId {
  MetricId::new(
    name.to_string().into(),
    mtype,
    tags
      .iter()
      .map(|(key, value)| TagValue {
        tag: (*key).to_string().into(),
        value: (*value).to_string().into(),
      })
      .collect(),
    false,
  )
  .unwrap()
}

// TODO(mattklein123): Redo all of this into some kind of builder.

pub fn make_metric(name: &str, tags: &[(&str, &str)], timestamp: u64) -> ParsedMetric {
  make_metric_ex(
    name,
    tags,
    timestamp,
    None,
    None,
    MetricValue::Simple(0.0),
    MetricSource::PromRemoteWrite,
    DownstreamId::LocalOrigin,
    None,
  )
}

pub fn make_metric_with_metadata(
  name: &str,
  tags: &[(&str, &str)],
  timestamp: u64,
  metadata: Metadata,
) -> ParsedMetric {
  make_metric_ex(
    name,
    tags,
    timestamp,
    None,
    None,
    MetricValue::Simple(0.0),
    MetricSource::PromRemoteWrite,
    DownstreamId::LocalOrigin,
    Some(metadata),
  )
}

pub fn make_counter(name: &str, tags: &[(&str, &str)], timestamp: u64, value: f64) -> ParsedMetric {
  make_metric_ex(
    name,
    tags,
    timestamp,
    Some(MetricType::Counter(CounterType::Delta)),
    None,
    MetricValue::Simple(value),
    MetricSource::PromRemoteWrite,
    DownstreamId::LocalOrigin,
    None,
  )
}

pub fn make_abs_counter(
  name: &str,
  tags: &[(&str, &str)],
  timestamp: u64,
  value: f64,
) -> ParsedMetric {
  make_metric_ex(
    name,
    tags,
    timestamp,
    Some(MetricType::Counter(CounterType::Absolute)),
    None,
    MetricValue::Simple(value),
    MetricSource::PromRemoteWrite,
    DownstreamId::LocalOrigin,
    None,
  )
}

pub fn make_abs_counter_with_metadata(
  name: &str,
  tags: &[(&str, &str)],
  timestamp: u64,
  value: f64,
  metadata: Metadata,
) -> ParsedMetric {
  make_metric_ex(
    name,
    tags,
    timestamp,
    Some(MetricType::Counter(CounterType::Absolute)),
    None,
    MetricValue::Simple(value),
    MetricSource::PromRemoteWrite,
    DownstreamId::LocalOrigin,
    Some(metadata),
  )
}

pub fn make_gauge(name: &str, tags: &[(&str, &str)], timestamp: u64, value: f64) -> ParsedMetric {
  make_metric_ex(
    name,
    tags,
    timestamp,
    Some(MetricType::Gauge),
    None,
    MetricValue::Simple(value),
    MetricSource::PromRemoteWrite,
    DownstreamId::LocalOrigin,
    None,
  )
}

pub fn make_gauge_with_metadata(
  name: &str,
  tags: &[(&str, &str)],
  timestamp: u64,
  value: f64,
  metadata: Metadata,
) -> ParsedMetric {
  make_metric_ex(
    name,
    tags,
    timestamp,
    Some(MetricType::Gauge),
    None,
    MetricValue::Simple(value),
    MetricSource::PromRemoteWrite,
    DownstreamId::LocalOrigin,
    Some(metadata),
  )
}

pub fn make_metric_ex(
  name: &str,
  tags: &[(&str, &str)],
  timestamp: u64,
  metric_type: Option<MetricType>,
  sample_rate: Option<f64>,
  value: MetricValue,
  metric_source: MetricSource,
  downstream_id: DownstreamId,
  metadata: Option<Metadata>,
) -> ParsedMetric {
  let mut metric = ParsedMetric::new(
    Metric::new(
      make_metric_id(name, metric_type, tags),
      sample_rate,
      timestamp,
      value,
    ),
    metric_source,
    Instant::now(),
    downstream_id,
  );
  if let Some(metadata) = metadata {
    metric.set_metadata(Some(Arc::new(metadata)));
  }
  metric
}

// TODO(mattklein123): Consider using mock time instead of this.
#[must_use]
pub fn clean_timestamps(metrics: Vec<ParsedMetric>) -> Vec<ParsedMetric> {
  metrics
    .into_iter()
    .map(|m| {
      ParsedMetric::new(
        Metric::new(
          m.metric().get_id().clone(),
          m.metric().sample_rate,
          0,
          m.metric().value.clone(),
        ),
        m.source().clone(),
        m.received_at(),
        m.downstream_id().clone(),
      )
    })
    .collect()
}

#[must_use]
pub fn make_carbon_wire_protocol() -> WireProtocol {
  WireProtocol {
    protocol_type: Some(Protocol_type::Carbon(Carbon::default())),
    ..Default::default()
  }
}

#[must_use]
pub fn parse_carbon_metrics(metrics: &[&str]) -> Vec<ParsedMetric> {
  let metric_cache = MetricCache::new(&Collector::default().scope("test"), None);
  metrics
    .iter()
    .map(|m| {
      let mut parsed_metric = ParsedMetric::try_from_wire_protocol(
        (*m).trim().to_string().into(),
        &make_carbon_wire_protocol(),
        Instant::now(),
        DownstreamId::LocalOrigin,
      )
      .unwrap();
      parsed_metric.initialize_cache(&metric_cache);
      parsed_metric
    })
    .collect()
}

//
// ProcessorFactoryContextHelper
//

pub struct ProcessorFactoryContextHelper {
  pub stats_helper: StatsHelper,
  pub dispatcher: Arc<MockPipelineDispatch>,
  pub metric_cache: Arc<MetricCache>,
  pub shutdown_trigger: ComponentShutdownTrigger,
  pub admin: Arc<MockAdmin>,
  pub k8s_sender: watch::Sender<PodsInfo>,
}

#[must_use]
pub fn processor_factory_context_for_test()
-> (ProcessorFactoryContextHelper, ProcessorFactoryContext) {
  let stats_helper = StatsHelper::default();
  let shutdown_trigger = ComponentShutdownTrigger::default();
  let shutdown_trigger_handle = shutdown_trigger.make_handle();
  let dispatcher = Arc::new(MockPipelineDispatch::new());
  let metric_cache = MetricCache::new(&stats_helper.collector().scope("metric_cache"), None);
  let scope = stats_helper.collector().scope("processor");
  let admin = Arc::new(MockAdmin::default());
  let (k8s_sender, k8s_receiver) = watch::channel(PodsInfo::default());
  (
    ProcessorFactoryContextHelper {
      stats_helper,
      dispatcher: dispatcher.clone(),
      metric_cache: metric_cache.clone(),
      shutdown_trigger,
      admin: admin.clone(),
      k8s_sender,
    },
    ProcessorFactoryContext {
      name: "test".to_string(),
      scope,
      metric_cache,
      dispatcher,
      shutdown_trigger_handle,
      singleton_manager: Arc::new(SingletonManager::default()),
      admin,
      bind_resolver: Arc::new(MockBindResolver::new()),
      time_provider: Box::<TestTimeProvider>::default(),
      k8s_watch_factory: Arc::new(move || {
        let cloned_k8s_receiver = k8s_receiver.clone();
        async move { Ok(Arc::new(PodsInfoSingleton::new(cloned_k8s_receiver))) }.boxed()
      }),
    },
  )
}

//
// FsConfigSwapHelper
//

pub struct FsConfigSwapHelper {
  temp_dir: TempDir,
  index: u64,
}

impl FsConfigSwapHelper {
  fn create_dir_and_file(&self, file_contents: &str) {
    let data_dir_path = self.temp_dir.path().join(format!("data_dir{}", self.index));
    fs::create_dir(&data_dir_path).unwrap();

    let data_file_path = data_dir_path.join("config.yaml");
    log::trace!("writing new config to: {}", data_file_path.display());
    let mut data_file = fs::File::create(data_file_path).unwrap();
    data_file.write_all(file_contents.as_bytes()).unwrap();
  }

  #[must_use]
  pub fn new(initial_contents: &str) -> Self {
    let mut helper = Self {
      temp_dir: TempDir::new().unwrap(),
      index: 0,
    };
    helper.create_dir_and_file(initial_contents);

    os_fs::symlink(
      helper.temp_dir.path().join("data_dir0"),
      helper.temp_dir.path().join("..data"),
    )
    .unwrap();
    os_fs::symlink(
      helper.temp_dir.path().join("..data").join("config.yaml"),
      helper.temp_dir.path().join("config.yaml"),
    )
    .unwrap();

    helper.index += 1;
    helper
  }

  #[must_use]
  pub fn path(&self) -> &Path {
    self.temp_dir.path()
  }

  pub fn update_config(&mut self, new_contents: &str) {
    self.create_dir_and_file(new_contents);
    os_fs::symlink(
      self.temp_dir.path().join(format!("data_dir{}", self.index)),
      self.temp_dir.path().join("..data.new"),
    )
    .unwrap();
    fs::rename(
      self.temp_dir.path().join("..data.new"),
      self.temp_dir.path().join("..data"),
    )
    .unwrap();
    self.index += 1;
  }
}
