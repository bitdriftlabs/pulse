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
  std::env::set_var("LOG_PANIC", "true");

  if std::env::var("ENABLE_TOKIO_CONSOLE").is_ok() {
    console_subscriber::init();
  }
}
