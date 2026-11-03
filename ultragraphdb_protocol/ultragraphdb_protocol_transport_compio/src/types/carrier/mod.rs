/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod carrier_async_read;
mod carrier_async_write;

use compio::net::{TcpStream, UnixStream};
use ultragraphdb_protocol_transport::{Address, Scheme, TransportError};

/// A connection's byte stream, its kind chosen by the [`Address`] it was dialled at or accepted on: a TCP socket or a
/// Unix-domain socket, read and written alike, so a [`Connection`](crate::Connection) and a
/// [`CompioTlsStream`](crate::CompioTlsStream) run over either without asking which.
#[derive(Debug)]
pub enum Carrier {
    /// A TCP socket, `tcp://host:port`.
    Tcp(TcpStream),
    /// A Unix-domain socket, `unix:///path`.
    Unix(UnixStream),
}

impl Carrier {
    /// Dials `address` over the carrier its scheme names.
    ///
    /// # Errors
    ///
    /// [`TransportError::io`] when the connection cannot be opened: no listener, a name that does not resolve, a path
    /// the platform cannot hold.
    pub async fn connect(address: &Address) -> Result<Self, TransportError> {
        let opened = match address.scheme() {
            Scheme::Tcp => TcpStream::connect(address.target()).await.map(Self::Tcp),
            Scheme::Unix => UnixStream::connect(address.target()).await.map(Self::Unix),
        };
        opened.map_err(|e| TransportError::io(&e))
    }
}
