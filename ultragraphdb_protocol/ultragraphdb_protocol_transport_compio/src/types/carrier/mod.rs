/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod carrier_async_read;
mod carrier_async_write;
mod carrier_splittable;

use std::net::IpAddr;

use compio::net::{TcpStream, UnixStream};
use socket2::SockRef;
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
    /// Dials `address` over the carrier its scheme names, with TCP keepalive on and each frame leaving at once
    /// (`PROTOCOL.md` §11.4). A future dropped while it dials closes the socket it opened, and leaves nothing.
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
        let carrier = opened.map_err(|e| TransportError::io(&e))?;
        carrier.keep_alive()?;
        carrier.no_delay()?;
        Ok(carrier)
    }

    /// Turns TCP keepalive on, so a peer that vanished is noticed (`PROTOCOL.md` §11.4). A Unix-domain socket's peer leaves no
    /// silence to probe, so nothing is set on one.
    ///
    /// # Errors
    ///
    /// [`TransportError::io`] when the socket refuses the option.
    pub fn keep_alive(&self) -> Result<(), TransportError> {
        match self {
            Self::Tcp(stream) => SockRef::from(stream)
                .set_keepalive(true)
                .map_err(|e| TransportError::io(&e)),
            Self::Unix(_) => Ok(()),
        }
    }

    /// Whether TCP keepalive is on; a Unix-domain socket has none.
    ///
    /// # Errors
    ///
    /// [`TransportError::io`] when the socket cannot say.
    pub fn is_kept_alive(&self) -> Result<bool, TransportError> {
        match self {
            Self::Tcp(stream) => SockRef::from(stream)
                .keepalive()
                .map_err(|e| TransportError::io(&e)),
            Self::Unix(_) => Ok(false),
        }
    }

    /// Has each frame leave as it is written (`PROTOCOL.md` §11.4): Nagle's algorithm would hold it behind an unacknowledged one,
    /// and a peer with nothing to send acknowledges only when its delayed-ACK timer fires, tens of milliseconds on.
    ///
    /// # Errors
    ///
    /// [`TransportError::io`] when the socket refuses the option.
    pub fn no_delay(&self) -> Result<(), TransportError> {
        match self {
            Self::Tcp(stream) => stream.set_nodelay(true).map_err(|e| TransportError::io(&e)),
            Self::Unix(_) => Ok(()),
        }
    }

    /// Whether a frame written leaves at once, TCP's `TCP_NODELAY`; a Unix-domain socket holds nothing back and has no such option.
    ///
    /// # Errors
    ///
    /// [`TransportError::io`] when the socket cannot say.
    pub fn is_no_delay(&self) -> Result<bool, TransportError> {
        match self {
            Self::Tcp(stream) => stream.nodelay().map_err(|e| TransportError::io(&e)),
            Self::Unix(_) => Ok(false),
        }
    }

    /// The address of the peer over TCP; `None` over a Unix-domain socket, whose peer is on this host, or a socket that
    /// cannot say.
    #[must_use]
    pub fn peer_ip(&self) -> Option<IpAddr> {
        match self {
            Self::Tcp(stream) => stream.peer_addr().ok().map(|peer| peer.ip()),
            Self::Unix(_) => None,
        }
    }
}
