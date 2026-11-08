/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;

use ultragraphdb_transport_tls::TlsConnector;

use crate::traits::connect::Connect;
use crate::types::tokio_tls_stream::TokioTlsStream;

/// Connections of `C` wrapped in TLS: each stream verified by the [`TlsConnector`] for one host, its handshake completed
/// before the wire protocol's first frame. A certificate that does not verify fails the connection, whose error names it.
#[derive(Debug, Clone)]
pub struct TlsDial<C> {
    inner: C,
    tls: TlsConnector,
    host: String,
}

impl<C> TlsDial<C> {
    /// The connections of `inner` verified by `tls` for `host`, a DNS name or an IP address.
    pub fn new(inner: C, tls: TlsConnector, host: impl Into<String>) -> Self {
        Self {
            inner,
            tls,
            host: host.into(),
        }
    }
}

impl<C: Connect> Connect for TlsDial<C> {
    type Stream = TokioTlsStream<C::Stream>;

    /// A connection of `inner`, its TLS handshake completed. A future dropped while it dials or runs the handshake
    /// drops the stream it opened, which closes it, and leaves nothing.
    async fn connect(&mut self) -> io::Result<Self::Stream> {
        let stream = self.inner.connect().await?;
        TokioTlsStream::connect(&self.tls, stream, &self.host)
            .await
            .map_err(io::Error::other)
    }
}
