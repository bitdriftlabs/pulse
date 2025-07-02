// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use crate::vrl::PulseDynamicState;
use prometheus::IntCounter;
use vrl::core::Value;
use vrl::prelude::{
  ArgumentList,
  Compiled,
  Context,
  Example,
  Expression,
  ExpressionError,
  Function,
  FunctionCompileContext,
  FunctionExpression,
  Parameter,
  Resolved,
  TypeDef,
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

    let Some(dynamic_state): Option<&PulseDynamicState> = ctx.get_external_context() else {
      return Err(Box::new(ExpressionError::from(
        "dynamic state must be PulseDynamicState",
      )));
    };
    let counter = dynamic_state.scope.counter(&name);

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
struct IncCounterFn {
  counter: IntCounter,
  value: Box<dyn Expression>,
}

impl FunctionExpression for IncCounterFn {
  fn resolve(&self, ctx: &mut Context<'_>) -> Resolved {
    let value = self.value.resolve(ctx)?.try_integer()?;
    self.counter.inc_by(
      value
        .try_into()
        .map_err(|_| ExpressionError::from("value must be an integer"))?,
    );
    Ok(Value::Null)
  }

  fn type_def(&self, _: &state::TypeState) -> TypeDef {
    TypeDef::null().infallible().impure()
  }
}
