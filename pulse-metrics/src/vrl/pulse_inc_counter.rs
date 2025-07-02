// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::vrl::PulseDynamicState;
use itertools::Itertools;
use prometheus::{IntCounter, IntCounterVec};
use vrl::core::Value;
use vrl::prelude::{
  ArgumentList,
  Compiled,
  Context,
  DiagnosticMessage,
  Example,
  Expression,
  ExpressionError,
  Function,
  FunctionCompileContext,
  FunctionExpression,
  Parameter,
  Resolved,
  TypeDef,
  ValueError,
  VrlValueConvert,
  kind,
  state,
};

#[derive(Debug)]
pub struct PulseIncCounter;

impl Function for PulseIncCounter {
  fn identifier(&self) -> &'static str {
    "pulse_inc_counter"
  }

  fn parameters(&self) -> &'static [Parameter] {
    &[
      Parameter {
        keyword: "name",
        kind: kind::ANY,
        required: true,
      },
      Parameter {
        keyword: "value",
        kind: kind::INTEGER,
        required: true,
      },
      Parameter {
        keyword: "tag_names",
        kind: kind::ARRAY,
        required: false,
      },
      Parameter {
        keyword: "tag_values",
        kind: kind::ARRAY,
        required: false,
      },
    ]
  }

  fn examples(&self) -> &'static [Example] {
    &[]
  }

  fn compile(
    &self,
    state: &state::TypeState,
    ctx: &mut FunctionCompileContext,
    arguments: ArgumentList,
  ) -> Compiled {
    let name = arguments.required("name");
    let Some(name) = name.resolve_constant(state) else {
      return Err(Box::new(ExpressionError::from(
        "name must be a constant string",
      )));
    };
    let name = name.to_string_lossy();
    let value = arguments.required("value");

    let tag_info = if let Some(tag_names) = arguments.optional("tag_names") {
      let Some(tag_names) = tag_names
        .resolve_constant(state)
        .and_then(|tag_names| tag_names.try_array().ok())
      else {
        return Err(Box::new(ExpressionError::from(
          "tag_names must be a constant array",
        )));
      };

      let tag_names: Vec<String> = tag_names
        .into_iter()
        .map(|tag_name| Ok::<_, ValueError>(tag_name.try_bytes_utf8_lossy()?.to_string()))
        .try_collect()
        .map_err(|_| {
          Box::new(ExpressionError::from(
            "tag_names must be a constant array of strings",
          )) as Box<dyn DiagnosticMessage>
        })?;

      if tag_names.is_empty() {
        return Err(Box::new(ExpressionError::from(
          "tag_names must not be an empty array",
        )));
      }

      let Some(tag_values) = arguments.optional("tag_values") else {
        return Err(Box::new(ExpressionError::from(
          "tag_values must be provided if tag_names are provided",
        )));
      };

      Some((tag_names, tag_values))
    } else {
      None
    };

    let Some(dynamic_state): Option<&PulseDynamicState> = ctx.get_external_context() else {
      return Err(Box::new(ExpressionError::from(
        "pulse_inc_counter can only be used in a pulse context",
      )));
    };

    let counter = if let Some((tag_names, tag_values)) = tag_info {
      CounterType::WithTags {
        counter: dynamic_state
          .scope
          .counter_vec_checked(&name, &tag_names.iter().map(String::as_str).collect_vec())
          .map_err(|_| {
            Box::new(ExpressionError::from(format!(
              "counter with name '{name}' is being registered with inconsistent tags"
            ))) as Box<dyn DiagnosticMessage>
          })?,
        num_tags: tag_names.len(),
        tag_values,
      }
    } else {
      CounterType::NoTags(dynamic_state.scope.counter_checked(&name).map_err(|_| {
        Box::new(ExpressionError::from(format!(
          "counter with name '{name}' is being registered with inconsistent tags"
        ))) as Box<dyn DiagnosticMessage>
      })?)
    };

    Ok(IncCounterFn { counter, value }.as_expr())
  }

  fn summary(&self) -> &'static str {
    ""
  }

  fn usage(&self) -> &'static str {
    ""
  }

  fn closure(&self) -> Option<vrl::prelude::closure::Definition> {
    None
  }
}

#[derive(Debug, Clone)]
enum CounterType {
  NoTags(IntCounter),
  WithTags {
    counter: IntCounterVec,
    num_tags: usize,
    tag_values: Box<dyn Expression>,
  },
}

#[derive(Debug, Clone)]
struct IncCounterFn {
  counter: CounterType,
  value: Box<dyn Expression>,
}

impl FunctionExpression for IncCounterFn {
  fn resolve(&self, ctx: &mut Context<'_>) -> Resolved {
    let value: u64 = self
      .value
      .resolve(ctx)?
      .try_integer()?
      .try_into()
      .map_err(|_| ExpressionError::from("value must be an integer"))?;
    match &self.counter {
      CounterType::NoTags(counter) => {
        counter.inc_by(value);
      },
      CounterType::WithTags {
        counter,
        num_tags,
        tag_values,
      } => {
        let tag_values = tag_values.resolve(ctx)?.try_array()?;
        if tag_values.len() != *num_tags {
          return Err(ExpressionError::from(format!(
            "tag_values must have exactly {} elements, got {}",
            num_tags,
            tag_values.len()
          )));
        }
        let tags: Vec<_> = tag_values
          .iter()
          .map(vrl::prelude::VrlValueConvert::try_bytes_utf8_lossy)
          .try_collect()
          .map_err(|_| ExpressionError::from("tag_values must be an array of strings"))?;
        counter.with_label_values(&tags).inc_by(value);
      },
    }
    Ok(Value::Null)
  }

  fn type_def(&self, _: &state::TypeState) -> TypeDef {
    TypeDef::null().fallible().impure()
  }
}
