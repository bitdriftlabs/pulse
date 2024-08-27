// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

use anyhow::anyhow;
use socket2::{Domain, Socket, Type};
use std::net::SocketAddr;
use tokio::net::{lookup_host, TcpListener, TcpSocket, UdpSocket};

pub struct BoundTcpSocket {
  socket: TcpSocket,
}

impl BoundTcpSocket {
  #[must_use]
  pub const fn new(socket: TcpSocket) -> Self {
    Self { socket }
  }

  #[must_use]
  pub fn listen(self) -> TcpListener {
    // Socket is bound. listen() cannot reasonably fail.
    self
      .socket
      .listen(1024)
      .expect("socket is bound and ready to listen")
  }

  #[must_use]
  pub fn local_addr(&self) -> SocketAddr {
    self.socket.local_addr().expect("socket is bound")
  }
}

// Trait for resolver a name to a listener. Used for test injection of sockets bound to port 0.
#[mockall::automock]
#[async_trait::async_trait]
pub trait BindResolver: Send + Sync {
  // Resolve the name and return a bound TCP socket, ready to listen(). Once bound, listen() cannot
  // reasonably fail under normal circumstances.
  async fn resolve_tcp(&self, name: &str) -> anyhow::Result<BoundTcpSocket>;

  // Resolve the name and return a bound UDP socket.
  async fn resolve_udp(&self, name: &str) -> anyhow::Result<UdpSocket>;
}

pub struct RealBindResolver {}

#[async_trait::async_trait]
impl BindResolver for RealBindResolver {
  async fn resolve_tcp(&self, name: &str) -> anyhow::Result<BoundTcpSocket> {
    make_reuse_port_tcp_socket(name).await
  }

  async fn resolve_udp(&self, name: &str) -> anyhow::Result<UdpSocket> {
    make_reuse_port_udp_socket(name).await
  }
}

pub async fn make_reuse_port_udp_socket(name: &str) -> anyhow::Result<UdpSocket> {
  let mut last_err = None;
  for addr in lookup_host(name).await? {
    let domain = match addr {
      SocketAddr::V4(_) => Domain::IPV4,
      SocketAddr::V6(_) => Domain::IPV6,
    };
    let socket = Socket::new(domain, Type::DGRAM, None)?;
    socket.set_reuse_port(true)?;
    socket.set_nonblocking(true)?;
    match socket.bind(&addr.into()) {
      Ok(()) => return Ok(UdpSocket::from_std(socket.into())?),
      Err(e) => last_err = Some(e.into()),
    }
  }

  Err(last_err.unwrap_or_else(|| anyhow!("could not resolve to any address")))
}

pub async fn make_reuse_port_tcp_socket(name: &str) -> anyhow::Result<BoundTcpSocket> {
  let mut last_err = None;
  for addr in lookup_host(name).await? {
    let socket = match addr {
      SocketAddr::V4(_) => TcpSocket::new_v4()?,
      SocketAddr::V6(_) => TcpSocket::new_v6()?,
    };
    socket.set_reuseport(true)?;
    match socket.bind(addr) {
      Ok(()) => return Ok(BoundTcpSocket::new(socket)),
      Err(e) => last_err = Some(e.into()),
    }
  }

  Err(last_err.unwrap_or_else(|| anyhow!("could not resolve to any address")))
}
