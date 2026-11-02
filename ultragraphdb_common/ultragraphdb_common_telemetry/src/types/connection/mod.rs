/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod connection_read;
mod connection_write;

use std::net::TcpStream;

use ultragraphdb_transport_tls::TlsStream;

/// The connection one exchange of [`OtlpHttpClient`](crate::OtlpHttpClient) runs over.
#[derive(Debug)]
pub(crate) enum Connection {
    /// `http://`: the socket itself.
    Plain(TcpStream),
    /// `https://`: a verified TLS connection over the socket, boxed because rustls's state is kilobytes where a
    /// socket is a handle.
    Tls(Box<TlsStream>),
}

impl Connection {
    /// The socket beneath, whose timeouts bound every read and write.
    pub(crate) fn socket(&self) -> &TcpStream {
        match self {
            Self::Plain(socket) => socket,
            Self::Tls(tls) => &tls.sock,
        }
    }
}
