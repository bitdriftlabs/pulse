// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use clap::Parser;
use pulse_vrl_tester::run;

#[derive(Parser)]
struct Options {
  #[arg(short = 'c', long = "config")]
  pub config: String,

  #[arg(long = "proxy-config")]
  pub proxy_config: Option<String>,
}

fn main() -> anyhow::Result<()> {
  let options = Options::parse();
  log::info!("loading test config from: {}", options.config);
  let config = std::fs::read_to_string(options.config)?;
  let proxy_config = options
    .proxy_config
    .map(|proxy_config| {
      log::info!("loading proxy config from: {proxy_config}");
      std::fs::read_to_string(proxy_config)
    })
    .transpose()?;
  run(&config, proxy_config.as_deref())
}
