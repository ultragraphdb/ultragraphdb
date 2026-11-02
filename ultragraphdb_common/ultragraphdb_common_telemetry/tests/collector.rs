/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! An in-memory stand-in for an OTLP collector and the dials that reach it, plain or over TLS: the client's
//! requests run on the test's own thread, or on the batch processor's, with no socket and no server thread.

use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use ultragraphdb_common_telemetry::Dial;
use ultragraphdb_transport_tls::{TestPeer, TestPki};

/// One connection to the collector: it records what the client sends and, once the request is complete, answers
/// with its response and then the end of the stream. A buffering connection sends what it was written only when
/// flushed, as a buffered writer does.
#[derive(Debug)]
pub struct Collector {
    received: Arc<Mutex<Vec<u8>>>,
    request: Vec<u8>,
    pending: Vec<u8>,
    buffering: bool,
    response: Arc<[u8]>,
    served: usize,
}

impl Collector {
    /// Moves `bytes` into the request the collector received.
    fn take(&mut self, bytes: &[u8]) {
        self.request.extend_from_slice(bytes);
        self.received
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(bytes);
    }
}

impl Write for Collector {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.buffering {
            self.pending.extend_from_slice(buf);
        } else {
            self.take(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let pending = std::mem::take(&mut self.pending);
        self.take(&pending);
        Ok(())
    }
}

impl Read for Collector {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !complete(&self.request) {
            return Ok(0);
        }
        let n = self
            .response
            .get(self.served..)
            .unwrap_or_default()
            .read(buf)?;
        self.served += n;
        Ok(n)
    }
}

/// Whether `request` holds its head and as many body bytes as its `Content-Length` announces.
fn complete(request: &[u8]) -> bool {
    let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") else {
        return false;
    };
    let head = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
    let length = head
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .map_or(0, |n| n.trim().parse::<usize>().unwrap_or(usize::MAX));
    request.len() >= end + 4 + length
}

/// Dials the collector over plain connections, recording each dial and every byte the client sends.
#[derive(Debug, Clone)]
pub struct CollectorDial {
    response: Arc<[u8]>,
    buffering: bool,
    // Shared with the test, which reads what the client sent after the batch processor's thread sent it.
    received: Arc<Mutex<Vec<u8>>>,
    dialed: Arc<Mutex<Vec<(String, u16, Duration)>>>,
}

impl CollectorDial {
    /// A collector answering every request with `response`.
    pub fn answering(response: impl Into<Vec<u8>>) -> Self {
        Self {
            response: response.into().into(),
            buffering: false,
            received: Arc::default(),
            dialed: Arc::default(),
        }
    }

    /// A collector as [`Self::answering`] gives it whose connections send only what was flushed.
    pub fn buffering(response: impl Into<Vec<u8>>) -> Self {
        Self {
            buffering: true,
            ..Self::answering(response)
        }
    }

    /// Every byte the client sent, as text.
    pub fn received(&self) -> String {
        let bytes = self.received.lock().unwrap_or_else(PoisonError::into_inner);
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// The host, port and timeout of every dial, in order.
    pub fn dialed(&self) -> Vec<(String, u16, Duration)> {
        self.dialed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Dial for CollectorDial {
    type Stream = Collector;

    fn dial(&self, host: &str, port: u16, timeout: Duration) -> io::Result<Collector> {
        self.dialed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((host.to_string(), port, timeout));
        Ok(Collector {
            received: Arc::clone(&self.received),
            request: Vec::new(),
            pending: Vec::new(),
            buffering: self.buffering,
            response: Arc::clone(&self.response),
            served: 0,
        })
    }
}

/// Dials the collector over TLS, the server side an in-memory [`TestPeer`] presenting `pki`'s certificate.
#[derive(Debug, Clone)]
pub struct TlsCollectorDial {
    pki: TestPki,
    plain: CollectorDial,
}

impl TlsCollectorDial {
    /// A collector behind `pki`'s certificate answering every request with `response`.
    pub fn answering(pki: &TestPki, response: impl Into<Vec<u8>>) -> Self {
        Self {
            pki: pki.clone(),
            plain: CollectorDial::answering(response),
        }
    }

    /// The collector behind the TLS server, for what it received and how it was dialed.
    pub fn plain(&self) -> &CollectorDial {
        &self.plain
    }
}

impl Dial for TlsCollectorDial {
    type Stream = TestPeer<Collector>;

    fn dial(&self, host: &str, port: u16, timeout: Duration) -> io::Result<TestPeer<Collector>> {
        Ok(self.pki.peer(self.plain.dial(host, port, timeout)?))
    }
}
