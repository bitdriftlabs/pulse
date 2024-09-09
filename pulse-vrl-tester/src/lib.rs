// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
mod test;

use anyhow::{anyhow, bail};
use config::bootstrap::v1::bootstrap::Config;
use config::processor::v1::processor::processor_config::Processor_type;
use pretty_assertions::Comparison;
use pulse_common::metadata::Metadata;
use pulse_common::proto::yaml_to_proto;
use pulse_metrics::protos::metric::{DownstreamId, Metric, MetricSource, ParsedMetric};
use pulse_metrics::protos::statsd;
use pulse_metrics::vrl::ProgramWrapper;
use pulse_protobuf::protos::pulse::config;
use pulse_protobuf::protos::pulse::vrl_tester::v1::vrl_tester::transform::Transform_type;
use pulse_protobuf::protos::pulse::vrl_tester::v1::vrl_tester::vrl_test_case::Program_type;
use pulse_protobuf::protos::pulse::vrl_tester::v1::vrl_tester::{VrlTestCase, VrlTesterConfig};
use std::sync::Arc;
use std::time::Instant;
use vrl::compiler::ExpressionError;

enum OutputType {
  Abort,
  Metric(Metric),
}

#[ctor::ctor]
fn global_init() {
  bd_log::SwapLogger::initialize();
}

fn run_test_case(test_case: VrlTestCase, proxy_config: Option<&Config>) -> anyhow::Result<usize> {
  let mut program_source: String = match test_case.program_type.as_ref().expect("pgv") {
    Program_type::Program(program) => Ok(program.as_str()),
    Program_type::MutateProcessorName(processor_name) => {
      let Some(config) = proxy_config else {
        bail!("mutate_processor_name requires passing a proxy config via --proxy-config");
      };
      config
        .pipeline()
        .processors
        .iter()
        .find_map(|(name, value)| {
          if name.as_str() == processor_name.as_str() {
            if let Some(Processor_type::Mutate(mutate)) = &value.processor_type {
              return Some(mutate.vrl_program.as_str());
            }
          }
          None
        })
        .ok_or_else(|| anyhow!("no mutate processor named '{processor_name} found in proxy config"))
    },
  }?
  .to_string();

  for (key, value) in test_case.program_replacements {
    program_source = program_source.replace(key.as_str(), &value);
  }

  let mut num_transforms = 0;
  let program = ProgramWrapper::new(&program_source)
    .map_err(|e| anyhow!("unable to compile VRL program '{}': {e}", program_source))?;
  let metadata = test_case
    .kubernetes_metadata
    .into_option()
    .map(|kubernetes_metadata| {
      Arc::new(Metadata::new(
        &kubernetes_metadata.namespace,
        &kubernetes_metadata.pod_name,
        &kubernetes_metadata
          .pod_labels
          .into_iter()
          .map(|(k, v)| (k.to_string(), v.to_string()))
          .collect(),
        &kubernetes_metadata
          .pod_annotations
          .into_iter()
          .map(|(k, v)| (k.to_string(), v.to_string()))
          .collect(),
        kubernetes_metadata.service_name.as_deref(),
      ))
    });

  for (key, value) in test_case.environment {
    std::env::set_var(key, value);
  }

  for transform in test_case.transforms {
    num_transforms += 1;
    match transform.transform_type {
      Some(Transform_type::Metric(metric_transform)) => {
        // TODO(mattklein123): Support parsing other formats. Probably a limited PromQL query of the
        // metric?
        let mut input = statsd::parse(&metric_transform.input.clone().into_bytes(), false)
          .map_err(|e| {
            anyhow!(
              "unable to parse input '{}' as statsd: {e}",
              metric_transform.input
            )
          })?;
        log::debug!("parsed input metric: {input}");
        input.timestamp = 0;
        let mut parsed_input = ParsedMetric::new(
          input,
          MetricSource::PromRemoteWrite,
          Instant::now(),
          DownstreamId::LocalOrigin,
        );
        parsed_input.set_metadata(metadata.clone());

        let output = if metric_transform.output.as_str() == "abort" {
          OutputType::Abort
        } else {
          let mut output = statsd::parse(&metric_transform.output.clone().into_bytes(), false)
            .map_err(|e| {
              anyhow!(
                "unable to parse output '{}' as statsd: {e}",
                metric_transform.output
              )
            })?;
          output.timestamp = 0;
          OutputType::Metric(output)
        };

        match (program.run_with_metric(&mut parsed_input), output) {
          (Ok(_) | Err(ExpressionError::Return { .. }), OutputType::Metric(output)) => {
            if &output != parsed_input.metric() {
              bail!(
                "VRL program '{}' failed to transform '{}' into '{}': {}",
                program_source,
                metric_transform.input,
                metric_transform.output,
                Comparison::new(&output, parsed_input.metric())
              );
            }
          },
          (Ok(_) | Err(ExpressionError::Return { .. }), OutputType::Abort) => {
            bail!(
              "VRL program '{}' failed to transform '{}' into abort/drop, got '{}'",
              program_source,
              metric_transform.input,
              parsed_input.metric()
            );
          },
          (Err(ExpressionError::Abort { .. }), OutputType::Abort) => {},
          (Err(ExpressionError::Abort { .. }), OutputType::Metric(_)) => {
            bail!(
              "VRL program '{}' unexpectedly transformed '{}' into abort/drop",
              program_source,
              metric_transform.input,
            );
          },
          (Err(e), _) => {
            bail!(
              "VRL program '{}' failed to run on input '{}': {e}",
              program_source,
              metric_transform.input
            );
          },
        }
      },
      None => unreachable!("pgv"),
    }
  }

  Ok(num_transforms)
}

pub fn run(config: &str, proxy_config: Option<&str>) -> anyhow::Result<()> {
  let config: VrlTesterConfig = yaml_to_proto(config)?;
  let proxy_config: Option<Config> = proxy_config.map(yaml_to_proto).transpose()?;

  let num_test_cases = config.test_cases.len();
  let mut num_transforms = 0;
  for test_case in config.test_cases {
    num_transforms += run_test_case(test_case, proxy_config.as_ref())?;
  }
  log::info!("processed {num_test_cases} test case(s) and {num_transforms} test transform(s)");

  Ok(())
}
