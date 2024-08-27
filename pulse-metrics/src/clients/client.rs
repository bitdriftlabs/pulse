// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use std::time::Instant;
use time::Duration;
use tokio::io::{AsyncWrite, AsyncWriteExt};

#[derive(Clone, Debug)]
pub enum ConnectTo {
  TcpSocketAddr(String),
  UnixPath(String),
}

// Client is a super simple wrapper over a connection, which destroys the
// connection if a write error occurs
pub struct Client {
  pub connect_to: ConnectTo,
  pub connection: Option<Box<dyn AsyncWrite + Send + Sync + Unpin>>,
  write_timeout: Option<Duration>,
  last_write: Instant,
}

impl Client {
  #[must_use]
  pub fn new(connect_to: ConnectTo, write_timeout: Option<Duration>) -> Self {
    Self {
      connection: None,
      connect_to,
      write_timeout,
      last_write: Instant::now(),
    }
  }

  pub async fn connect(&mut self) -> std::io::Result<()> {
    self.connection = Some(match &self.connect_to {
      ConnectTo::TcpSocketAddr(addr) => Box::new(tokio::net::TcpStream::connect(addr).await?),
      ConnectTo::UnixPath(path) => Box::new(tokio::net::UnixStream::connect(path).await?),
    });
    self.last_write = Instant::now();
    Ok(())
  }

  #[must_use]
  pub fn is_connected(&self) -> bool {
    self.connection.is_some()
  }

  /// A socket can be in a `FIN_WAIT_2` state if the server closed the socket. In that case,
  /// the next socket write will silently fail. We add a write timeout to mitigate this issue
  /// as best as we can.
  /// See: <https://users.rust-lang.org/t/tcpstream-write-silently-loses-one-message/38206>
  #[must_use]
  pub fn is_stale(&self) -> bool {
    self
      .write_timeout
      .is_some_and(|write_timeout| self.last_write.elapsed() > write_timeout)
  }

  pub async fn write<T: bytes::Buf>(&mut self, from: &mut T) -> std::io::Result<()> {
    match self.connection.as_mut() {
      Some(connection) => match connection.write_all_buf(from).await {
        Err(e) => {
          self.connection = None;
          Err(e)
        },
        Ok(o) => {
          self.last_write = Instant::now();
          Ok(o)
        },
      },
      None => Err(std::io::ErrorKind::NotConnected.into()),
    }
  }
}
