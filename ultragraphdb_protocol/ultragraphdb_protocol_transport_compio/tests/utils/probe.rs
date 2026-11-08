/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A simulated stream that counts what is asked of it: the bytes written, the flushes and the shutdowns, its two halves
//! counting together.

use std::cell::Cell;
use std::rc::Rc;

use compio::BufResult;
use compio::buf::{IoBuf, IoBufMut};
use compio::io::util::Splittable;
use compio::io::{AsyncRead, AsyncWrite};
use ultragraphdb_protocol_transport::{NetworkFaults, SimNetwork};
use ultragraphdb_protocol_transport_compio::{
    CompioSimStream, CompioTlsStream, TlsReadHalf, TlsWriteHalf,
};
use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_transport_tls::TestPki;

use crate::utils::ready::ready;
use crate::utils::sim::connect;

/// What a [`Probe`] was asked to do.
#[derive(Debug, Default)]
pub struct Probed {
    /// The bytes written.
    pub written: Cell<usize>,
    /// The flushes.
    pub flushes: Cell<u32>,
    /// The shutdowns.
    pub shutdowns: Cell<u32>,
}

/// A simulated stream counting into `probed`.
#[derive(Debug)]
pub struct Probe {
    inner: CompioSimStream,
    probed: Rc<Probed>,
}

impl Probe {
    /// `inner`, counting into `probed`.
    pub fn new(inner: CompioSimStream, probed: Rc<Probed>) -> Self {
        Self { inner, probed }
    }
}

impl AsyncRead for Probe {
    async fn read<B: IoBufMut>(&mut self, buf: B) -> BufResult<usize, B> {
        self.inner.read(buf).await
    }
}

impl AsyncWrite for Probe {
    async fn write<T: IoBuf>(&mut self, buf: T) -> BufResult<usize, T> {
        let BufResult(written, buf) = self.inner.write(buf).await;
        if let Ok(n) = written {
            self.probed.written.set(self.probed.written.get() + n);
        }
        BufResult(written, buf)
    }

    async fn flush(&mut self) -> std::io::Result<()> {
        self.probed.flushes.set(self.probed.flushes.get() + 1);
        self.inner.flush().await
    }

    async fn shutdown(&mut self) -> std::io::Result<()> {
        self.probed.shutdowns.set(self.probed.shutdowns.get() + 1);
        self.inner.shutdown().await
    }
}

impl Splittable for Probe {
    type ReadHalf = Self;
    type WriteHalf = Self;

    fn split(self) -> (Self, Self) {
        let (read, write) = self.inner.split();
        (
            Self::new(read, Rc::clone(&self.probed)),
            Self::new(write, self.probed),
        )
    }
}

/// The halves a split TLS client stream over a [`Probe`]: the server's whole stream at the other end, the client's read
/// half and write half, their handshake completed, and what the probe counted.
pub type SplitPair = (
    CompioTlsStream<CompioSimStream>,
    TlsReadHalf<Probe>,
    TlsWriteHalf<Probe>,
    Rc<Probed>,
);

/// A TLS connection of `pki`'s over the simulated network, its client split once the handshake completed.
pub fn split_pair(pki: &TestPki) -> TestResult<SplitPair> {
    let network = SimNetwork::new(1, NetworkFaults::none());
    let (client, server) = connect(&network);
    let probed = Rc::new(Probed::default());
    let (acceptor, connector) = (pki.acceptor()?, pki.connector()?);
    let (server, client) = ready(futures::future::join(
        CompioTlsStream::accept(&acceptor, server),
        CompioTlsStream::connect(
            &connector,
            Probe::new(client, Rc::clone(&probed)),
            "localhost",
        ),
    ));
    let (read, write) = client?.split();
    Ok((server?, read, write, probed))
}
