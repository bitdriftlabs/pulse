// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
mod test;

use anyhow::{anyhow, bail};
use bd_server_stats::stats::Collector;
use config::bootstrap::v1::bootstrap::Config;
use config::processor::v1::processor::processor_config::Processor_type;
use protobuf::Message;
use pulse_common::proto::yaml_to_proto;
use pulse_metrics::pipeline::processor::drop::TranslatedDropConfig;
use pulse_metrics::protos::metric::{DownstreamId, MetricSource, ParsedMetric};
use pulse_metrics::protos::statsd;
use pulse_protobuf::protos::pulse::config;
use pulse_protobuf::protos::pulse::config::common::v1::common::wire_protocol::StatsD;
use pulse_protobuf::protos::pulse::config::processor::v1::drop::DropConfig;
use pulse_protobuf::protos::pulse::config::processor::v1::drop::drop_processor_config::Config_source;
use pulse_protobuf::protos::pulse::config::processor::v1::processor::ProcessorConfig;
use pulse_protobuf::protos::pulse::drop_tester::v1::drop_tester::drop_test_case::Config_type;
use pulse_protobuf::protos::pulse::drop_tester::v1::drop_tester::{DropTestCase, DropTesterConfig};
use std::time::Instant;

#[ctor::ctor]
fn global_init() {
  bd_log::SwapLogger::initialize();
}

fn run_test_case(test_case: DropTestCase, proxy_config: Option<&Config>) -> anyhow::Result<usize> {
  fn extract_from_config(
    field_name: &str,
    proxy_config: Option<&Config>,
    processor_name: &str,
    extract: impl Fn(&ProcessorConfig) -> Option<DropConfig>,
  ) -> anyhow::Result<DropConfig> {
    let Some(config) = proxy_config else {
      bail!("{field_name} requires passing a proxy config via --proxy-config");
    };
    config
      .pipeline()
      .processors
      .iter()
      .find_map(|(name, value)| {
        if name.as_str() == processor_name {
          return extract(value);
        }
        None
      })
      .ok_or_else(|| anyhow!("no processor named '{processor_name} found in proxy config"))
  }

  let drop_config: DropConfig = match test_case.config_type.as_ref().expect("pgv") {
    Config_type::Config(config) => Ok(config.clone()),
    Config_type::DropProcessorName(processor_name) => extract_from_config(
      "mutate_processor_name",
      proxy_config,
      processor_name,
      |value| {
        if let Some(Processor_type::Drop(drop)) = &value.processor_type {
          return Some(match drop.config_source.as_ref().expect("pgv") {
            Config_source::Inline(config) => config.clone(),
            Config_source::FileSource(_) => {
              // TODO(mattklein123): Support file source if needed.
              return None;
            },
          });
        }

        None
      },
    ),
  }?;

  let drop_config = TranslatedDropConfig::new(&drop_config, &Collector::default().scope("test"))?;

  let mut num_metrics = 0;
  for metric in test_case.metrics {
    num_metrics += 1;

    // TODO(mattklein123): Support parsing other formats. Probably a limited PromQL query of the
    // metric?
    let mut input = statsd::parse(
      &metric.input.clone().into_bytes(),
      StatsD::default_instance(),
    )
    .map_err(|e| anyhow!("unable to parse input '{}' as statsd: {e}", metric.input))?;
    log::debug!("parsed input metric: {input}");
    input.timestamp = 0;
    let parsed_input = ParsedMetric::new(
      input,
      MetricSource::PromRemoteWrite,
      Instant::now(),
      DownstreamId::LocalOrigin,
    );

    let dropped_by = drop_config.drop_sample(&parsed_input).unwrap_or("");
    if metric.dropped_by.as_str() != dropped_by {
      bail!(
        "expected metric '{}' to be dropped by '{}' but actually dropped by '{}'",
        metric.input,
        metric.dropped_by,
        dropped_by
      );
    }
  }

  Ok(num_metrics)
}

pub fn run(config: &str, proxy_config: Option<&str>) -> anyhow::Result<()> {
  let config: DropTesterConfig = yaml_to_proto(config)?;
  let proxy_config: Option<Config> = proxy_config.map(yaml_to_proto).transpose()?;

  let num_test_cases = config.test_cases.len();
  let mut num_metrics = 0;
  for test_case in config.test_cases {
    num_metrics += run_test_case(test_case, proxy_config.as_ref())?;
  }
  log::info!("processed {num_test_cases} test case(s) and {num_metrics} test metrics(s)");

  Ok(())
}
