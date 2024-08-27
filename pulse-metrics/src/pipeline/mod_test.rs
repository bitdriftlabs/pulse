// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::{MetricPipeline, MockItemFactory, PipelineDispatch};
use crate::admin::test::MockAdmin;
use crate::pipeline::inflow::MockPipelineInflow;
use crate::pipeline::outflow::MockPipelineOutflow;
use crate::pipeline::processor::MockPipelineProcessor;
use crate::protos::metric::ParsedMetric;
use crate::test::make_gauge;
use bd_server_stats::test::util::stats::Helper as StatsHelper;
use bd_shutdown::ComponentShutdownTriggerHandle;
use bd_test_helpers::make_mut;
use parking_lot::Mutex;
use pulse_common::bind_resolver::MockBindResolver;
use pulse_common::proto::yaml_to_proto;
use pulse_common::singleton::SingletonManager;
use pulse_protobuf::protos::pulse::config::bootstrap::v1::bootstrap::{
  KubernetesBootstrapConfig,
  PipelineConfig,
};
use std::collections::HashMap;
use std::sync::Arc;

struct InflowState {
  inflow: Arc<MockPipelineInflow>,
  dispatcher: Option<Arc<dyn PipelineDispatch>>,
  shutdown_trigger_handle: Option<ComponentShutdownTriggerHandle>,
}

struct ProcessorState {
  processor: Arc<MockPipelineProcessor>,
  dispatcher: Option<Arc<dyn PipelineDispatch>>,
  shutdown_trigger_handle: Option<ComponentShutdownTriggerHandle>,
}

struct OutflowState {
  outflow: Arc<MockPipelineOutflow>,
  shutdown_trigger_handle: Option<ComponentShutdownTriggerHandle>,
}

#[derive(Clone, Copy)]
enum ExpectStart {
  No,
  Yes,
}

#[derive(Default)]
struct Helper {
  metric_pipeline: Option<MetricPipeline>,
  factory: Arc<MockItemFactory>,
  inflows: HashMap<String, Vec<Arc<Mutex<Option<InflowState>>>>>,
  processors: HashMap<String, Vec<Arc<Mutex<Option<ProcessorState>>>>>,
  outflows: HashMap<String, Vec<Arc<Mutex<Option<OutflowState>>>>>,
}

impl Helper {
  async fn create(&mut self, config_yaml: &str) {
    assert!(self.metric_pipeline.is_none());

    let pipeline_config: PipelineConfig = yaml_to_proto(config_yaml).unwrap();
    let stats_helper = StatsHelper::default();
    self.metric_pipeline = Some(
      MetricPipeline::new_from_config(
        self.factory.clone(),
        stats_helper.collector().scope("test"),
        KubernetesBootstrapConfig::default(),
        Arc::new(|| unreachable!()),
        pipeline_config,
        Arc::new(SingletonManager::default()),
        Arc::new(MockAdmin::default()),
        Arc::new(MockBindResolver::new()),
      )
      .await
      .unwrap(),
    );
  }

  async fn update(&self, config_yaml: &str) -> anyhow::Result<()> {
    let pipeline_config: PipelineConfig = yaml_to_proto(config_yaml).unwrap();
    self
      .metric_pipeline
      .as_ref()
      .unwrap()
      .update_config(pipeline_config)
      .await
  }

  fn expect_make_inflow(&mut self, name: &'static str, expect_start: ExpectStart) {
    let mut inflow = Arc::new(MockPipelineInflow::new());
    if !matches!(expect_start, ExpectStart::No) {
      make_mut(&mut inflow)
        .expect_start()
        .times(1)
        .return_once(move || match expect_start {
          ExpectStart::Yes => (),
          ExpectStart::No => unreachable!(),
        });
    }
    let state = Arc::new(Mutex::new(Some(InflowState {
      inflow,
      dispatcher: None,
      shutdown_trigger_handle: None,
    })));
    self
      .inflows
      .entry(name.to_string())
      .or_default()
      .push(state.clone());
    make_mut(&mut self.factory)
      .expect_to_inflow()
      .times(1)
      .return_once(move |_, c| {
        assert_eq!(c.name, name);
        let mut state = state.lock();
        let state = state.as_mut().unwrap();
        state.dispatcher = Some(c.dispatcher);
        state.shutdown_trigger_handle = Some(c.shutdown_trigger_handle);
        Ok(state.inflow.clone())
      });
  }

  fn expect_make_processor(&mut self, name: &'static str, expect_start: ExpectStart) {
    let mut processor = Arc::new(MockPipelineProcessor::new());
    if !matches!(expect_start, ExpectStart::No) {
      make_mut(&mut processor)
        .expect_start()
        .times(1)
        .return_once(move || {
          Box::pin(async move {
            match expect_start {
              ExpectStart::Yes => (),
              ExpectStart::No => unreachable!(),
            }
          })
        });
    }
    let state = Arc::new(Mutex::new(Some(ProcessorState {
      processor,
      dispatcher: None,
      shutdown_trigger_handle: None,
    })));
    self
      .processors
      .entry(name.to_string())
      .or_default()
      .push(state.clone());
    make_mut(&mut self.factory)
      .expect_to_processor()
      .times(1)
      .return_once(move |_, c| {
        assert_eq!(c.name, name);
        let mut state = state.lock();
        let state = state.as_mut().unwrap();
        state.dispatcher = Some(c.dispatcher);
        state.shutdown_trigger_handle = Some(c.shutdown_trigger_handle);
        Ok(state.processor.clone())
      });
  }

  fn expect_make_ouflow(&mut self, name: &'static str) {
    let outflow = Arc::new(MockPipelineOutflow::new());
    let state = Arc::new(Mutex::new(Some(OutflowState {
      outflow,
      shutdown_trigger_handle: None,
    })));
    self
      .outflows
      .entry(name.to_string())
      .or_default()
      .push(state.clone());
    make_mut(&mut self.factory)
      .expect_to_outflow()
      .times(1)
      .return_once(move |_, c| {
        assert_eq!(c.name, name);
        let mut state = state.lock();
        let state = state.as_mut().unwrap();
        state.shutdown_trigger_handle = Some(c.shutdown_trigger_handle);
        Ok(state.outflow.clone())
      });
  }

  async fn send_from_inflow(&self, name: &str, index: usize, samples: Vec<ParsedMetric>) {
    let dispatcher = {
      let state = self.inflows[name][index].lock();
      let state = state.as_ref().unwrap();
      assert!(state.shutdown_trigger_handle.is_some());
      state.dispatcher.as_ref().unwrap().clone()
    };
    dispatcher.send(samples).await;
  }

  fn expect_processor_recv(&self, name: &str, index: usize) {
    let mut state = self.processors[name][index].lock();
    let state = state.as_mut().unwrap();
    let dispatcher = state.dispatcher.as_ref().unwrap().clone();
    make_mut(&mut state.processor)
      .expect_recv_samples()
      .times(1)
      .return_once(move |samples| Box::pin(async move { dispatcher.send(samples).await }));
  }

  fn expect_outflow_recv(&self, name: &str, index: usize) {
    make_mut(&mut self.outflows[name][index].lock().as_mut().unwrap().outflow)
      .expect_recv_samples()
      .times(1)
      .return_once(|_| ());
  }

  fn expect_shutdown_inflow(&self, name: &str, index: usize) {
    let inflow_state = self.inflows[name][index].lock().take().unwrap();
    let mut cancellation = inflow_state
      .shutdown_trigger_handle
      .as_ref()
      .unwrap()
      .make_shutdown();
    tokio::spawn(async move {
      cancellation.cancelled().await;
    });
  }

  fn expect_shutdown_outflow(&self, name: &str, index: usize) {
    let outflow_state = self.outflows[name][index].lock().take().unwrap();
    let mut cancellation = outflow_state
      .shutdown_trigger_handle
      .as_ref()
      .unwrap()
      .make_shutdown();
    tokio::spawn(async move {
      cancellation.cancelled().await;
    });
  }

  fn expect_shutdown_processor(&self, name: &str, index: usize) {
    let processor_state = self.processors[name][index].lock().take().unwrap();
    let mut cancellation = processor_state
      .shutdown_trigger_handle
      .as_ref()
      .unwrap()
      .make_shutdown();
    tokio::spawn(async move {
      cancellation.cancelled().await;
    });
  }
}

#[tokio::test]
async fn no_change() {
  let mut helper = Helper::default();
  helper.expect_make_inflow("inflow1", ExpectStart::No);
  helper.expect_make_processor("processor1", ExpectStart::No);
  helper.expect_make_ouflow("outflow1");
  let config = r#"
inflows:
  inflow1:
    routes: ["processor:processor1"]
    tcp:
      bind: "foo"
      protocol:
        carbon: {}

processors:
  processor1:
    routes: ["outflow:outflow1"]
    buffer:
      max_buffered_metrics: 10
      num_consumers: 1

outflows:
  outflow1:
    tcp:
      common:
        send_to: "bar"
        protocol:
          carbon: {}
    "#;
  helper.create(config).await;
  helper.update(config).await.unwrap();

  helper.expect_processor_recv("processor1", 0);
  helper.expect_outflow_recv("outflow1", 0);
  helper
    .send_from_inflow("inflow1", 0, vec![make_gauge("foo", &[], 0, 0.0)])
    .await;
}

#[tokio::test]
async fn replace_outflow() {
  let mut helper = Helper::default();
  helper.expect_make_inflow("inflow1", ExpectStart::No);
  helper.expect_make_processor("processor1", ExpectStart::No);
  helper.expect_make_ouflow("outflow1");
  let config = r#"
inflows:
  inflow1:
    routes: ["processor:processor1"]
    tcp:
      bind: "foo"
      protocol:
        carbon: {}

processors:
  processor1:
    routes: ["outflow:outflow1"]
    buffer:
      max_buffered_metrics: 10
      num_consumers: 1

outflows:
  outflow1:
    tcp:
      common:
        send_to: "bar"
        protocol:
          carbon: {}
    "#;
  helper.create(config).await;

  helper.expect_make_ouflow("outflow1");
  let updated_config = r#"
inflows:
  inflow1:
    routes: ["processor:processor1"]
    tcp:
      bind: "foo"
      protocol:
        carbon: {}

processors:
  processor1:
    routes: ["outflow:outflow1"]
    buffer:
      max_buffered_metrics: 10
      num_consumers: 1

outflows:
  outflow1:
    tcp:
      common:
        send_to: "baz"
        protocol:
          carbon: {}
      "#;
  helper.expect_shutdown_outflow("outflow1", 0);
  helper.update(updated_config).await.unwrap();

  helper.expect_processor_recv("processor1", 0);
  helper.expect_outflow_recv("outflow1", 1);
  helper
    .send_from_inflow("inflow1", 0, vec![make_gauge("foo", &[], 0, 0.0)])
    .await;
}

#[tokio::test]
async fn replace_processor() {
  let mut helper = Helper::default();
  helper.expect_make_inflow("inflow1", ExpectStart::No);
  helper.expect_make_processor("processor1", ExpectStart::No);
  helper.expect_make_ouflow("outflow1");
  let config = r#"
inflows:
  inflow1:
    routes: ["processor:processor1"]
    tcp:
      bind: "foo"
      protocol:
        carbon: {}

processors:
  processor1:
    routes: ["outflow:outflow1"]
    buffer:
      max_buffered_metrics: 10
      num_consumers: 1

outflows:
  outflow1:
    tcp:
      common:
        send_to: "bar"
        protocol:
          carbon: {}
    "#;
  helper.create(config).await;

  let updated_config = r#"
inflows:
  inflow1:
    routes: ["processor:processor1"]
    tcp:
      bind: "foo"
      protocol:
        carbon: {}

processors:
  processor1:
    routes: ["outflow:outflow1"]
    buffer:
      max_buffered_metrics: 11
      num_consumers: 1

outflows:
  outflow1:
    tcp:
      common:
        send_to: "bar"
        protocol:
          carbon: {}
      "#;
  helper.expect_make_processor("processor1", ExpectStart::Yes);
  helper.expect_shutdown_processor("processor1", 0);
  helper.update(updated_config).await.unwrap();

  helper.expect_processor_recv("processor1", 1);
  helper.expect_outflow_recv("outflow1", 0);
  helper
    .send_from_inflow("inflow1", 0, vec![make_gauge("foo", &[], 0, 0.0)])
    .await;
}

#[tokio::test]
async fn replace_inflow() {
  let mut helper = Helper::default();
  helper.expect_make_inflow("inflow1", ExpectStart::No);
  helper.expect_make_processor("processor1", ExpectStart::No);
  helper.expect_make_ouflow("outflow1");
  let config = r#"
inflows:
  inflow1:
    routes: ["processor:processor1"]
    tcp:
      bind: "foo"
      protocol:
        carbon: {}

processors:
  processor1:
    routes: ["outflow:outflow1"]
    buffer:
      max_buffered_metrics: 10
      num_consumers: 1

outflows:
  outflow1:
    tcp:
      common:
        send_to: "bar"
        protocol:
          carbon: {}
    "#;
  helper.create(config).await;

  let updated_config = r#"
inflows:
  inflow1:
    routes: ["processor:processor1"]
    tcp:
      bind: "foo2"
      protocol:
        carbon: {}

processors:
  processor1:
    routes: ["outflow:outflow1"]
    buffer:
      max_buffered_metrics: 10
      num_consumers: 1

outflows:
  outflow1:
    tcp:
      common:
        send_to: "bar"
        protocol:
          carbon: {}
      "#;
  helper.expect_make_inflow("inflow1", ExpectStart::Yes);
  helper.expect_shutdown_inflow("inflow1", 0);
  helper.update(updated_config).await.unwrap();

  helper.expect_processor_recv("processor1", 0);
  helper.expect_outflow_recv("outflow1", 0);
  helper
    .send_from_inflow("inflow1", 1, vec![make_gauge("foo", &[], 0, 0.0)])
    .await;
}
