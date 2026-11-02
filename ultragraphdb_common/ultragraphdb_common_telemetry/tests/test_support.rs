/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The writer and the span exporter the unit tests and the scenarios install the subscriber over, and the one-shot
//! HTTP server the client and the OTLP exporter send to.

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};

use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::trace::{InMemorySpanExporter, SpanData, SpanExporter};

/// What the fmt layer writes, shared with the test.
#[derive(Clone, Default)]
pub struct Buffer(Arc<Mutex<Vec<u8>>>);

impl Buffer {
    pub fn text(&self) -> String {
        let bytes = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        String::from_utf8(bytes.clone()).expect("the fmt layer writes UTF-8")
    }
}

impl io::Write for Buffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut bytes = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The in-memory exporter, kept through the shutdown the guard's drop sends it: the SDK's own exporter clears its
/// spans on shutdown.
#[derive(Debug, Clone, Default)]
pub struct Kept(pub InMemorySpanExporter);

impl SpanExporter for Kept {
    async fn export(&self, batch: Vec<SpanData>) -> OTelSdkResult {
        self.0.export(batch).await
    }
}

/// A server on a loopback port that accepts one connection, reads one request, `Content-Length` body included,
/// writes `response` and closes: its base URL, and the thread returning the request's bytes.
#[allow(dead_code)]
pub fn serve_once(response: &'static [u8]) -> (String, JoinHandle<Vec<u8>>) {
    serve_once_over("http", response, |stream| stream)
}

/// A server as [`serve_once`] runs it, over TLS with `pki`'s certificate for `localhost`: its `https://localhost`
/// base URL, and the thread returning the request's bytes.
#[allow(dead_code)]
pub fn serve_once_tls(
    pki: &ultragraphdb_transport_tls::TestPki,
    response: &'static [u8],
) -> (String, JoinHandle<Vec<u8>>) {
    let pki = pki.clone();
    serve_once_over("https", response, move |stream| pki.accept(stream))
}

/// The one-shot server of `scheme`, its connection's stream made by `wrap`.
#[allow(
    dead_code,
    clippy::disallowed_methods,
    reason = "a test's stand-in collector serves on a thread of its own; the scenarios use none of it"
)]
fn serve_once_over<S: Read + Write>(
    scheme: &str,
    response: &'static [u8],
    wrap: impl FnOnce(TcpStream) -> S + Send + 'static,
) -> (String, JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let host = if scheme == "https" {
        "localhost"
    } else {
        "127.0.0.1"
    };
    let url = format!("{scheme}://{host}:{port}");
    let handle = thread::spawn(move || {
        let (socket, _) = listener.accept().expect("one connection");
        let mut stream = wrap(socket);
        let mut request = Vec::new();
        let mut buf = [0u8; 4096];
        while !complete(&request) {
            let n = stream.read(&mut buf).expect("the request");
            if n == 0 {
                break;
            }
            request.extend_from_slice(&buf[..n]);
        }
        stream.write_all(response).expect("the response");
        stream.flush().expect("the response is sent");
        request
    });
    (url, handle)
}

#[allow(dead_code, reason = "the scenarios use no stand-in collector")]
/// Whether `request` holds its head and as many body bytes as its `Content-Length` announces.
fn complete(request: &[u8]) -> bool {
    let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") else {
        return false;
    };
    let head = String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
    let length = head
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .map_or(0, |n| n.trim().parse::<usize>().expect("a length"));
    request.len() >= end + 4 + length
}
