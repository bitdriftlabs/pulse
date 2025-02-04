// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::protos::metric::{EditableParsedMetric, MetricType, ParsedMetric, TagValue};
use anyhow::{anyhow, bail};
use pulse_common::metadata::Metadata;
use vrl::compiler::state::{ExternalEnv, RuntimeState};
use vrl::compiler::{
  compile_with_external,
  CompileConfig,
  Context,
  OwnedValueOrRef,
  Program,
  Resolved,
  SecretTarget,
  Target,
  TimeZone,
};
use vrl::diagnostic::Formatter;
use vrl::path::{OwnedTargetPath, PathPrefix};
use vrl::value::kind::Collection;
use vrl::value::{Kind, Value};

//
// EditableMetricVrlTarget
//

#[derive(Debug)]
struct EditableMetricVrlTarget<'a> {
  metric: EditableParsedMetric<'a>,
}

impl<'a> EditableMetricVrlTarget<'a> {
  const fn new(metric: EditableParsedMetric<'a>) -> Self {
    Self { metric }
  }
}

impl SecretTarget for EditableMetricVrlTarget<'_> {
  fn get_secret(&self, _key: &str) -> Option<&str> {
    None
  }

  fn insert_secret(&mut self, _key: &str, _value: &str) {}

  fn remove_secret(&mut self, _key: &str) {}
}

impl Target for EditableMetricVrlTarget<'_> {
  fn target_insert(
    &mut self,
    path: &vrl::path::OwnedTargetPath,
    value: vrl::prelude::Value,
  ) -> Result<(), String> {
    // TODO(mattklein123): The runtime ignores errors from this function.
    match path.prefix {
      PathPrefix::Event => {
        log::trace!("inserting: path={path} value={value}");
        if path.path.is_root() {
          return Err("assigning to root event is not supported".to_string());
        }
        match path.path.to_alternative_components(3)[0].as_slice() {
          [name] if name == "name" => {
            self.metric.change_name(
              value
                .as_bytes()
                .ok_or_else(|| "assigning to name requires a string".to_string())?
                .clone(),
            );
          },
          [tags, tag_name] if tags == "tags" => {
            self.metric.add_or_change_tag(TagValue {
              tag: tag_name.to_bytes(),
              value: value
                .as_bytes()
                .ok_or_else(|| "assigning to a tag value requires a string".to_string())?
                .clone(),
            });
          },
          // TODO(mattklein123): Support other fields as well as assigning all tags.
          _ => return Ok(()),
        }

        Ok(())
      },
      PathPrefix::Metadata => Ok(()),
    }
  }

  fn target_get(
    &mut self,
    path: &vrl::path::OwnedTargetPath,
  ) -> Result<Option<OwnedValueOrRef<'_>>, String> {
    // TODO(mattklein123): The runtime ignores errors from this function so just return Ok(None)
    // per normal lookup even for not supported fields.
    match path.prefix {
      PathPrefix::Event => {
        if path.path.is_root() {
          // TODO(mattklein123): Create the root object on demand in the off case it is actually
          // asked for.
          return Ok(None);
        }
        match path.path.to_alternative_components(3)[0].as_slice() {
          [name] if name == "name" => Ok(Some(OwnedValueOrRef::Owned(Value::Bytes(
            self.metric.metric().metric().get_id().name().clone(),
          )))),
          [tags, tag_name] if tags == "tags" => Ok(
            self
              .metric
              .find_tag(tag_name.as_bytes())
              .map(|t| OwnedValueOrRef::Owned(Value::Bytes(t.value.clone()))),
          ),
          [name] if name == "mtype" => Ok(Some(OwnedValueOrRef::Owned(Value::Bytes(
            match self.metric.metric().metric().get_id().mtype() {
              None => "unknown",
              Some(MetricType::Counter(_)) => "counter",
              Some(MetricType::DeltaGauge | MetricType::DirectGauge | MetricType::Gauge) => "gauge",
              Some(MetricType::Histogram) => "histogram",
              Some(MetricType::Summary) => "summary",
              Some(MetricType::Timer | MetricType::BulkTimer) => "timer",
            }
            .into(),
          )))),
          _ => {
            // TODO(mattklein123): Support synthetic reading of other attributes on demand.
            Ok(None)
          },
        }
      },
      PathPrefix::Metadata => Ok(
        self
          .metric
          .metric()
          .metadata()
          .as_ref()
          .and_then(|m| m.value().get(&path.path))
          .map(OwnedValueOrRef::Ref),
      ),
    }
  }

  fn target_remove(
    &mut self,
    path: &vrl::path::OwnedTargetPath,
    _compact: bool,
  ) -> Result<Option<vrl::prelude::Value>, String> {
    // TODO(mattklein123): The runtime ignores errors from this function.
    match path.prefix {
      PathPrefix::Event => {
        log::trace!("removing: path={path}");
        let removed = match path.path.to_alternative_components(3)[0].as_slice() {
          [tags, tag_name] if tags == "tags" => self
            .metric
            .delete_tag(tag_name.as_bytes())
            .map(Value::Bytes),
          // TODO(mattklein123): Per above figure out some way of indicating an error for anything
          // that is not a tag removal.
          _ => None,
        };

        Ok(removed)
      },
      PathPrefix::Metadata => Ok(None),
    }
  }
}


//
// MetadataTargetWrapper
//

#[derive(Debug)]
struct MetadataTargetWrapper<'a> {
  metadata: Option<&'a Metadata>,
}

impl SecretTarget for MetadataTargetWrapper<'_> {
  fn get_secret(&self, _key: &str) -> Option<&str> {
    None
  }

  fn insert_secret(&mut self, _key: &str, _value: &str) {}

  fn remove_secret(&mut self, _key: &str) {}
}

impl Target for MetadataTargetWrapper<'_> {
  fn target_insert(&mut self, _path: &OwnedTargetPath, _value: Value) -> Result<(), String> {
    Ok(())
  }

  fn target_get(&mut self, path: &OwnedTargetPath) -> Result<Option<OwnedValueOrRef<'_>>, String> {
    match path.prefix {
      PathPrefix::Event => Ok(None),
      PathPrefix::Metadata => Ok(
        self
          .metadata
          .and_then(|m| m.value().get(&path.path))
          .map(OwnedValueOrRef::Ref),
      ),
    }
  }

  fn target_remove(
    &mut self,
    _path: &vrl::path::OwnedTargetPath,
    _compact: bool,
  ) -> Result<Option<Value>, String> {
    Ok(None)
  }
}

//
// ProgramWrapper
//

pub struct ProgramWrapper {
  program: Program,
}

impl ProgramWrapper {
  pub fn new(program: &str) -> anyhow::Result<Self> {
    let external = ExternalEnv::new_with_kind(
      Kind::object(
        Collection::empty()
          .with_known("name", Kind::bytes())
          .with_known(
            "tags",
            Kind::object(Collection::empty().with_unknown(Kind::bytes())),
          )
          .with_known("mtype", Kind::bytes()),
      ),
      Metadata::schema(),
    );

    let result = compile_with_external(
      program,
      &vrl::stdlib::all(),
      &external,
      CompileConfig::default(),
    )
    .map_err(|e| anyhow!("VRL compile error: {}", Formatter::new(program, e)))?;
    if !result.warnings.is_empty() {
      bail!(Formatter::new(program, result.warnings).to_string(),);
    }

    Ok(Self {
      program: result.program,
    })
  }

  pub fn run_with_metric(&self, sample: &mut ParsedMetric) -> Resolved {
    let mut state = RuntimeState::default();
    let timezone = TimeZone::default();
    let mut target = EditableMetricVrlTarget::new(EditableParsedMetric::new(sample));
    let mut ctx = Context::new(&mut target, &mut state, &timezone);
    self.program.resolve(&mut ctx)
  }

  pub fn run_with_metadata(&self, metadata: Option<&Metadata>) -> Resolved {
    let mut state = RuntimeState::default();
    let timezone = TimeZone::default();
    let mut target = MetadataTargetWrapper { metadata };
    let mut ctx = Context::new(&mut target, &mut state, &timezone);
    self.program.resolve(&mut ctx)
  }
}
