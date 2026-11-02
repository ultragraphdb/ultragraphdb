/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! One HTTP/1.1 exchange over a `std::net` connection, plain or verified TLS: the request with `Content-Length`
//! and `Connection: close`, the response read to the connection's end and framed by `Transfer-Encoding: chunked`
//! or `Content-Length`.

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use opentelemetry_http::{Bytes, HttpError, Request, Response};
use ultragraphdb_transport_tls::TlsConnector;

use crate::types::connection::Connection;

/// The largest response read, as the exporter's built-in clients bound it.
const MAX_RESPONSE: usize = 4 * 1024 * 1024;

/// The request's headers this module writes itself, and so skips among the caller's.
const FRAMING: [&str; 4] = ["host", "content-length", "connection", "transfer-encoding"];

/// Sends `request` to its `http://` URI, or its `https://` URI through `tls`, and returns the response, all within
/// `timeout`: connecting, the TLS handshake, the request and the response's last byte.
pub(crate) fn exchange(
    request: &Request<Bytes>,
    timeout: Duration,
    tls: Option<&TlsConnector>,
) -> Result<Response<Bytes>, HttpError> {
    let deadline = now() + timeout;
    let uri = request.uri();
    let (connector, port_default) = match (uri.scheme_str(), tls) {
        (Some("http"), _) => (None, 80),
        (Some("https"), Some(connector)) => (Some(connector), 443),
        (Some("https"), None) => return Err(format!("https needs a TLS connector: {uri}").into()),
        _ => return Err(format!("http:// or https:// only: {uri}").into()),
    };
    let host = uri.host().ok_or_else(|| format!("no host in {uri}"))?;
    let port = uri.port_u16().unwrap_or(port_default);
    let path = uri.path_and_query().map_or("/", |p| p.as_str());
    let socket = connect(host, port, deadline)?;
    socket.set_write_timeout(Some(left(deadline)?))?;
    socket.set_read_timeout(Some(left(deadline)?))?;
    let mut connection = match connector {
        Some(connector) => Connection::Tls(Box::new(connector.connect(socket, bare(host))?)),
        None => Connection::Plain(socket),
    };
    connection.write_all(&head(request, host, port, port_default, path))?;
    connection.write_all(request.body())?;
    // TLS buffers what it writes; the flush sends it.
    connection.flush()?;
    parse(&read_to_end(&mut connection, deadline)?)
}

/// `host` without the brackets `Uri::host` keeps around an IPv6 literal, which neither the resolver nor a
/// certificate's names carry.
fn bare(host: &str) -> &str {
    host.trim_start_matches('[').trim_end_matches(']')
}

/// The time, read where the client meets the network.
#[allow(
    clippy::disallowed_methods,
    reason = "a request to a collector is bounded by the wall clock; no storage code reads it"
)]
fn now() -> Instant {
    Instant::now()
}

/// The time left before `deadline`, or the error that it has passed.
fn left(deadline: Instant) -> Result<Duration, HttpError> {
    deadline
        .checked_duration_since(now())
        .filter(|d| !d.is_zero())
        .ok_or_else(|| "timed out".into())
}

/// A connection to the first address of `host` that accepts one before `deadline`.
fn connect(host: &str, port: u16, deadline: Instant) -> Result<TcpStream, HttpError> {
    let mut last = format!("no address for {host}");
    for addr in (bare(host), port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&addr, left(deadline)?) {
            Ok(stream) => return Ok(stream),
            Err(e) => last = format!("{addr}: {e}"),
        }
    }
    Err(last.into())
}

/// The request line and headers: the framing this module owns, then the caller's other headers. The `Host` header
/// leaves out a port that is the scheme's default.
fn head(request: &Request<Bytes>, host: &str, port: u16, port_default: u16, path: &str) -> Vec<u8> {
    let authority = if port == port_default {
        host.to_string()
    } else {
        format!("{host}:{port}")
    };
    let mut head = format!(
        "{} {path} HTTP/1.1\r\nHost: {authority}\r\nContent-Length: {}\r\nConnection: close\r\n",
        request.method(),
        request.body().len()
    )
    .into_bytes();
    request
        .headers()
        .iter()
        .filter(|(name, _)| !FRAMING.contains(&name.as_str()))
        .for_each(|(name, value)| {
            head.extend_from_slice(name.as_str().as_bytes());
            head.extend_from_slice(b": ");
            head.extend_from_slice(value.as_bytes());
            head.extend_from_slice(b"\r\n");
        });
    head.extend_from_slice(b"\r\n");
    head
}

/// Every byte the server sends until it closes the connection, at most [`MAX_RESPONSE`], before `deadline`.
///
/// A TLS peer that closes its socket without a `close_notify` alert ends the stream too: rustls reports that as
/// `UnexpectedEof`, and a truncated response is still refused, by [`parse`], when its framing announces more.
fn read_to_end(connection: &mut Connection, deadline: Instant) -> Result<Vec<u8>, HttpError> {
    let mut raw = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        connection
            .socket()
            .set_read_timeout(Some(left(deadline)?))?;
        let n = match connection.read(&mut buf) {
            Ok(n) => n,
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => 0,
            Err(e) => return Err(e.into()),
        };
        if n == 0 {
            return Ok(raw);
        }
        raw.extend_from_slice(&buf[..n]);
        if raw.len() > MAX_RESPONSE {
            return Err(format!("response over {MAX_RESPONSE} bytes").into());
        }
    }
}

/// The response in `raw`: its status, its headers, and its body as its framing delimits it.
fn parse(raw: &[u8]) -> Result<Response<Bytes>, HttpError> {
    let end = find(raw, b"\r\n\r\n").ok_or("response without a complete head")?;
    let head = std::str::from_utf8(&raw[..end])?;
    let rest = &raw[end + 4..];
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    let status: u16 = status_line
        .split(' ')
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| format!("malformed status line {status_line:?}"))?;
    let mut builder = Response::builder().status(status);
    let (mut chunked, mut length) = (false, None);
    for line in lines {
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| format!("malformed header {line:?}"))?;
        let value = value.trim();
        if name.eq_ignore_ascii_case("transfer-encoding") {
            chunked = value.to_ascii_lowercase().contains("chunked");
        } else if name.eq_ignore_ascii_case("content-length") {
            length = Some(value.parse::<usize>()?);
        }
        builder = builder.header(name, value);
    }
    let body = match (chunked, length) {
        (true, _) => dechunk(rest)?,
        (false, Some(n)) => rest
            .get(..n)
            .ok_or_else(|| format!("body of {} bytes, {n} announced", rest.len()))?
            .to_vec(),
        (false, None) => rest.to_vec(),
    };
    Ok(builder.body(Bytes::from(body))?)
}

/// The body a chunked transfer coding carries in `data`, trailers ignored.
fn dechunk(mut data: &[u8]) -> Result<Vec<u8>, HttpError> {
    let mut body = Vec::new();
    loop {
        let eol = find(data, b"\r\n").ok_or("chunk size without its line end")?;
        let line = std::str::from_utf8(&data[..eol])?;
        let hex = line.split(';').next().unwrap_or_default().trim();
        let size =
            usize::from_str_radix(hex, 16).map_err(|_| format!("malformed chunk size {line:?}"))?;
        data = &data[eol + 2..];
        if size == 0 {
            return Ok(body);
        }
        let chunk = data.get(..size).ok_or("chunk shorter than its size")?;
        body.extend_from_slice(chunk);
        data = data
            .get(size..)
            .and_then(|d| d.strip_prefix(b"\r\n"))
            .ok_or("chunk without its line end")?;
    }
}

/// The offset of `needle` in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}
