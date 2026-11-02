/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod connection_read;
mod connection_write;

use std::io::{Read, Write};

use ultragraphdb_transport_tls::TlsStream;

/// The connection one exchange of [`OtlpHttpClient`](crate::OtlpHttpClient) runs over, on a dialed stream `S`.
#[derive(Debug)]
pub(crate) enum Connection<S: Read + Write> {
    /// `http://`: the stream itself.
    Plain(S),
    /// `https://`: a verified TLS connection over the stream, boxed because rustls's state is kilobytes where a
    /// socket is a handle.
    Tls(Box<TlsStream<S>>),
}
