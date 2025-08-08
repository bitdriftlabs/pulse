// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use log::Level;
use parking_lot::Mutex;
use tokio::time::Instant;
use vrl::core::Value;
use vrl::parser::Span;
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
use vrl::value;

#[derive(Debug)]
pub struct PulseLog;

impl Function for PulseLog {
  fn identifier(&self) -> &'static str {
    "pulse_log"
  }

  fn parameters(&self) -> &'static [Parameter] {
    &[
      Parameter {
        keyword: "value",
        kind: kind::ANY,
        required: true,
      },
      Parameter {
        keyword: "level",
        kind: kind::BYTES,
        required: false,
      },
      Parameter {
        keyword: "rate_limit_secs",
        kind: kind::INTEGER,
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
    let levels = vec![
      "trace".into(),
      "debug".into(),
      "info".into(),
      "warn".into(),
      "error".into(),
    ];

    let value = arguments.required("value");
    let level = arguments
      .optional_enum("level", &levels, state)?
      .unwrap_or_else(|| "info".into())
      .try_bytes()
      .expect("log level not bytes");

    // TODO(mattklein123): Either require a constant for this or try to resolve constant lookup
    // which will be the common case.
    let rate_limit_secs = arguments.optional("rate_limit_secs");

    let level = match level.as_ref() {
      b"trace" => Level::Trace,
      b"debug" => Level::Debug,
      b"warn" => Level::Warn,
      b"error" => Level::Error,
      _ => Level::Info,
    };

    Ok(
      LogFn {
        span: ctx.span(),
        value,
        level,
        rate_limit_secs,
        last_emit_time: Mutex::default(),
      }
      .as_expr(),
    )
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

#[derive(Debug)]
struct LogFn {
  span: Span,
  value: Box<dyn Expression>,
  level: Level,
  rate_limit_secs: Option<Box<dyn Expression>>,
  last_emit_time: Mutex<Option<Instant>>,
}

impl Clone for LogFn {
  fn clone(&self) -> Self {
    Self {
      span: self.span,
      value: self.value.clone(),
      level: self.level,
      rate_limit_secs: self.rate_limit_secs.clone(),
      last_emit_time: Mutex::new(*self.last_emit_time.lock()),
    }
  }
}

impl FunctionExpression for LogFn {
  fn resolve(&self, ctx: &mut Context<'_>) -> Resolved {
    let value = self.value.resolve(ctx)?;
    let rate_limit_secs = match &self.rate_limit_secs {
      Some(expr) => expr.resolve(ctx)?,
      None => value!(1),
    };

    let rate_limit_secs = rate_limit_secs.try_integer()?;
    {
      let mut last_emit_time = self.last_emit_time.lock();
      if let Some(last_time) = *last_emit_time
        && last_time.elapsed().as_secs()
          < rate_limit_secs.try_into().map_err(|_| {
            ExpressionError::from("rate_limit_secs must be an integer that can be converted to u64")
          })?
      {
        return Ok(Value::Null);
      }
      *last_emit_time = Some(Instant::now());
    }

    let res = value.to_string_lossy();
    log::log!(self.level, "{res}");
    Ok(Value::Null)
  }

  fn type_def(&self, _: &state::TypeState) -> TypeDef {
    TypeDef::null().infallible().impure()
  }
}
