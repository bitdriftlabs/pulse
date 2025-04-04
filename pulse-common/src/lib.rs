// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

pub mod bind_resolver;
pub mod k8s;
pub mod metadata;
pub mod proto;
pub mod singleton;

use bd_log::SwapLogger;
use bd_panic::PanicType;

#[cfg(test)]
#[ctor::ctor]
fn test_global_init() {
  global_initialize();
}

pub fn global_initialize() {
  // Call this before we initilize the logger as there is an issue where a log emitted /w thread
  // ids (always set by SwapLogger) emitted during ctor will panic.
  // See https://github.com/tokio-rs/tracing/issues/2063#issuecomment-2024185427, ideally this will
  // be resolved somehow.
  bd_panic::default(PanicType::ForceAbort);

  SwapLogger::initialize();

  rustls::crypto::aws_lc_rs::default_provider()
    .install_default()
    .unwrap();

  // We don't control the environment where the proxy is run so for now just force LOG_PANIC on
  // release builds.
  #[cfg(not(debug_assertions))]
  unsafe {
    std::env::set_var("LOG_PANIC", "true");
  }

  if std::env::var("ENABLE_TOKIO_CONSOLE").is_ok() {
    console_subscriber::init();
  }
}

pub trait LossyIntoToFloat {
  fn lossy_to_f64(self) -> f64;
}

impl LossyIntoToFloat for u64 {
  #[allow(clippy::cast_precision_loss)]
  fn lossy_to_f64(self) -> f64 {
    self as f64
  }
}

impl LossyIntoToFloat for usize {
  #[allow(clippy::cast_precision_loss)]
  fn lossy_to_f64(self) -> f64 {
    self as f64
  }
}

pub trait LossyFloatToInt {
  fn lossy_to_usize(self) -> usize;
  fn lossy_to_u64(self) -> u64;
}

impl LossyFloatToInt for f64 {
  #[allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
  )]
  fn lossy_to_usize(self) -> usize {
    self as usize
  }

  #[allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
  )]
  fn lossy_to_u64(self) -> u64 {
    self as u64
  }
}
