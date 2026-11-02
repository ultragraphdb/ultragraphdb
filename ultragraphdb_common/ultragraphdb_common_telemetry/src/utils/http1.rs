/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! One HTTP/1.1 exchange over a `std::net` connection: the request with `Content-Length` and `Connection: close`,
//! the response read to the connection's end and framed by `Transfer-Encoding: chunked` or `Content-Length`.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use opentelemetry_http::{Bytes, HttpError, Request, Response};

/// The largest response read, as the exporter's built-in clients bound it.
const MAX_RESPONSE: usize = 4 * 1024 * 1024;

/// The request's headers this module writes itself, and so skips among the caller's.
const FRAMING: [&str; 4] = ["host", "content-length", "connection", "transfer-encoding"];

/// Sends `request` to its `http://` URI and returns the response, all within `timeout`.
pub(crate) fn exchange(
    request: &Request<Bytes>,
    timeout: Duration,
) -> Result<Response<Bytes>, HttpError> {
    let deadline = now() + timeout;
    let uri = request.uri();
    if uri.scheme_str() != Some("http") {
        return Err(format!("plain HTTP only: {uri}").into());
    }
    let host = uri.host().ok_or_else(|| format!("no host in {uri}"))?;
    let port = uri.port_u16().unwrap_or(80);
    let path = uri.path_and_query().map_or("/", |p| p.as_str());
    let mut stream = connect(host, port, deadline)?;
    stream.set_write_timeout(Some(left(deadline)?))?;
    stream.write_all(&head(request, host, port, path))?;
    stream.write_all(request.body())?;
    parse(&read_to_end(&mut stream, deadline)?)
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
    // `Uri::host` keeps an IPv6 literal's brackets, which the resolver does not take.
    let name = host.trim_start_matches('[').trim_end_matches(']');
    let mut last = format!("no address for {host}");
    for addr in (name, port).to_socket_addrs()? {
        match TcpStream::connect_timeout(&addr, left(deadline)?) {
            Ok(stream) => return Ok(stream),
            Err(e) => last = format!("{addr}: {e}"),
        }
    }
    Err(last.into())
}

/// The request line and headers: the framing this module owns, then the caller's other headers.
fn head(request: &Request<Bytes>, host: &str, port: u16, path: &str) -> Vec<u8> {
    let authority = if port == 80 {
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
fn read_to_end(stream: &mut TcpStream, deadline: Instant) -> Result<Vec<u8>, HttpError> {
    let mut raw = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        stream.set_read_timeout(Some(left(deadline)?))?;
        let n = stream.read(&mut buf)?;
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
