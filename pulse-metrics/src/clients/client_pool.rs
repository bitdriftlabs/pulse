// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use super::client::{Client, ConnectTo};
// Re-export the pool
pub use deadpool::managed::reexports::*;
use deadpool::managed::{self, QueueMode};
use time::Duration;
deadpool::managed_reexports!(
  "carbon_client",
  ClientManager,
  deadpool::managed::Object<ClientManager>,
  std::io::Error,
  std::io::Error
);
use time::ext::NumericalStdDuration;

#[must_use]
pub fn new(connect_to: ConnectTo, write_timeout: Option<Duration>) -> Pool {
  let manager = ClientManager::new(connect_to, write_timeout);
  let config = PoolConfig {
    max_size: 1024,
    timeouts: Timeouts {
      wait: Some(100.std_milliseconds()),
      create: Some(10.std_seconds()),
      recycle: None,
    },
    queue_mode: QueueMode::Fifo,
  };
  Pool::builder(manager)
    .config(config)
    .runtime(Runtime::Tokio1)
    .build()
    .unwrap()
}

#[derive(Debug)]
pub struct ClientManager {
  connect_to: ConnectTo,
  write_timeout: Option<Duration>,
}

impl ClientManager {
  #[must_use]
  pub const fn new(connect_to: ConnectTo, write_timeout: Option<Duration>) -> Self {
    Self {
      connect_to,
      write_timeout,
    }
  }
}

impl managed::Manager for ClientManager {
  type Type = Client;
  type Error = std::io::Error;

  async fn create(&self) -> Result<Client, std::io::Error> {
    let mut client = Client::new(self.connect_to.clone(), self.write_timeout);
    client.connect().await?;
    Ok(client)
  }

  async fn recycle(
    &self,
    client: &mut Client,
    _: &Metrics,
  ) -> managed::RecycleResult<std::io::Error> {
    match (client.is_connected(), client.is_stale()) {
      (true, false) => Ok(()),
      _ => Err(managed::RecycleError::Backend(
        std::io::ErrorKind::NotConnected.into(),
      )),
    }
  }
}
