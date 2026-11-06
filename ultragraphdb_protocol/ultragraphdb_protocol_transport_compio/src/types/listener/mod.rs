/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use compio::net::{TcpListener, UnixListener};
use ultragraphdb_protocol_transport::{Address, AddressRule, Scheme, TransportError};

use crate::types::carrier::Carrier;

/// Where a role takes connections, its kind chosen by the scheme of the [`Address`] it was bound to: a TCP listener or
/// a Unix-domain socket's.
#[derive(Debug)]
pub enum Listener {
    /// Listening at `tcp://host:port`.
    Tcp(TcpListener),
    /// Listening at `unix:///path`.
    Unix(UnixListener),
}

impl Listener {
    /// Listens at `address`. A Unix address's socket file is created here, and binding fails if a file is at its
    /// path already.
    ///
    /// # Errors
    ///
    /// [`TransportError::io`] when the address cannot be bound: in use, not permitted, a path the platform cannot
    /// hold.
    pub async fn bind(address: &Address) -> Result<Self, TransportError> {
        let bound = match address.scheme() {
            Scheme::Tcp => TcpListener::bind(address.target()).await.map(Self::Tcp),
            Scheme::Unix => UnixListener::bind(address.target()).await.map(Self::Unix),
        };
        bound.map_err(|e| TransportError::io(&e))
    }

    /// The next connection, a carrier of the listener's kind, with TCP keepalive on (`PROTOCOL.md` §11.4).
    ///
    /// # Errors
    ///
    /// [`TransportError::io`] when accepting fails or the socket refuses the option.
    pub async fn accept(&self) -> Result<Carrier, TransportError> {
        let accepted = match self {
            Self::Tcp(listener) => listener
                .accept()
                .await
                .map(|(stream, _)| Carrier::Tcp(stream)),
            Self::Unix(listener) => listener
                .accept()
                .await
                .map(|(stream, _)| Carrier::Unix(stream)),
        };
        let carrier = accepted.map_err(|e| TransportError::io(&e))?;
        carrier.keep_alive()?;
        Ok(carrier)
    }

    /// The address the listener holds: a TCP listener's with its port, the one the system chose for port 0.
    ///
    /// # Errors
    ///
    /// [`TransportError::io`] when the socket cannot say, and [`TransportError::address`] for a Unix socket bound to
    /// no path.
    pub fn local_address(&self) -> Result<Address, TransportError> {
        let io = |e: std::io::Error| TransportError::io(&e);
        match self {
            Self::Tcp(listener) => {
                Address::parse(&format!("tcp://{}", listener.local_addr().map_err(io)?))
            }
            Self::Unix(listener) => {
                let bound = listener.local_addr().map_err(io)?;
                let path = bound.as_pathname().and_then(|path| path.to_str());
                let Some(path) = path else {
                    return Err(TransportError::address("unix://", AddressRule::UnixUnbound));
                };
                Address::parse(&format!("unix://{path}"))
            }
        }
    }
}
