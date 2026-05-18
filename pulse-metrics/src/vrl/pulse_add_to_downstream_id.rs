// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/licenses/strict/1.0.0.txt

use crate::vrl::RuntimeDynamicData;
use vrl::compiler::value::VrlValueConvert;
use vrl::core::Value;
use vrl::prelude::{
  ArgumentList,
  Compiled,
  Context,
  Example,
  Expression,
  Function,
  FunctionCompileContext,
  FunctionExpression,
  Parameter,
  Resolved,
  TypeDef,
  kind,
  state,
};

#[derive(Debug)]
pub struct PulseAddToDownstreamId;

impl Function for PulseAddToDownstreamId {
  fn identifier(&self) -> &'static str {
    "pulse_add_to_downstream_id"
  }

  fn parameters(&self) -> &'static [Parameter] {
    &[Parameter {
      keyword: "suffix",
      kind: kind::BYTES,
      required: true,
    }]
  }

  fn examples(&self) -> &'static [Example] {
    &[]
  }

  fn compile(
    &self,
    _state: &state::TypeState,
    _ctx: &mut FunctionCompileContext,
    arguments: ArgumentList,
  ) -> Compiled {
    let suffix = arguments.required("suffix");
    Ok(AddToDownstreamIdFn { suffix }.as_expr())
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
struct AddToDownstreamIdFn {
  suffix: Box<dyn Expression>,
}

impl FunctionExpression for AddToDownstreamIdFn {
  fn resolve(&self, ctx: &mut Context<'_>) -> Resolved {
    let suffix = self.suffix.resolve(ctx)?.try_bytes()?;

    // Empty suffix is a no-op.
    if suffix.is_empty() {
      return Ok(Value::Null);
    }

    let Some(dynamic_data) = ctx
      .dynamic_state()
      .and_then(|d| d.downcast_mut::<RuntimeDynamicData>())
    else {
      // This shouldn't happen in normal use, but if there's no dynamic data, just ignore.
      return Ok(Value::Null);
    };

    dynamic_data.downstream_id_suffix = Some(suffix);
    Ok(Value::Null)
  }

  fn type_def(&self, _: &state::TypeState) -> TypeDef {
    TypeDef::null().fallible().impure()
  }
}
