/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;
use std::net::SocketAddr;

use compio::net::TcpStream;

use crate::traits::connect::Connect;

/// Connections over TCP to one address. `tcp_nodelay` is on by default: the socket's `TCP_NODELAY` option is set,
/// which disables Nagle's algorithm, since every frame is written whole and waiting to coalesce it only delays it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TcpConnector {
    address: SocketAddr,
    tcp_nodelay: bool,
}

impl TcpConnector {
    /// Connections to `address`, `tcp_nodelay` on.
    #[must_use]
    pub fn new(address: SocketAddr) -> Self {
        Self {
            address,
            tcp_nodelay: true,
        }
    }

    /// Whether each connection sets `TCP_NODELAY`, disabling Nagle's algorithm: `true`, the default, or `false` to
    /// clear it, leaving Nagle's algorithm on.
    #[must_use]
    pub fn with_tcp_nodelay(mut self, tcp_nodelay: bool) -> Self {
        self.tcp_nodelay = tcp_nodelay;
        self
    }

    /// Whether each connection sets `TCP_NODELAY`.
    #[must_use]
    pub fn tcp_nodelay(&self) -> bool {
        self.tcp_nodelay
    }

    /// The address connected to.
    #[must_use]
    pub fn address(&self) -> SocketAddr {
        self.address
    }
}

impl Connect for TcpConnector {
    type Stream = TcpStream;

    async fn connect(&mut self) -> io::Result<TcpStream> {
        let stream = TcpStream::connect(self.address).await?;
        stream.set_nodelay(self.tcp_nodelay)?;
        Ok(stream)
    }
}
