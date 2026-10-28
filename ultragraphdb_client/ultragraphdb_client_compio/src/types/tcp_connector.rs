/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;
use std::net::SocketAddr;

use compio::net::TcpStream;

use crate::traits::connect::Connect;

/// Connections over TCP to one address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TcpConnector {
    address: SocketAddr,
}

impl TcpConnector {
    pub fn new(address: SocketAddr) -> Self {
        Self { address }
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }
}

impl Connect for TcpConnector {
    type Stream = TcpStream;

    async fn connect(&mut self) -> io::Result<TcpStream> {
        TcpStream::connect(self.address).await
    }
}
