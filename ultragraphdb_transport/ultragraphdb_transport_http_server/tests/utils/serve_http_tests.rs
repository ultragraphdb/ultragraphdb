/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use compio::BufResult;
use compio::io::{AsyncReadExt, AsyncWrite, AsyncWriteExt};
use compio::net::{TcpListener, TcpStream};
use compio::runtime::{Runtime, spawn};
use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_transport_http_server::{
    HttpLimits, HttpRequest, HttpResponse, HttpService, serve_http,
};

#[derive(Clone, Copy)]
struct Fine;

impl HttpService for Fine {
    async fn call(&self, _request: HttpRequest) -> HttpResponse {
        HttpResponse::new(200).with_body("fine")
    }
}

/// A connection to a server of `limits`.
async fn connected(limits: HttpLimits) -> TestResult<TcpStream> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            serve_http(stream, &limits, &Fine).await;
        }
    })
    .detach();
    Ok(TcpStream::connect(address).await?)
}

async fn answer(stream: &mut TcpStream) -> TestResult<String> {
    let BufResult(read, bytes) = stream.read_to_end(Vec::new()).await;
    let _count: usize = read?;
    Ok(String::from_utf8(bytes)?)
}

/// A request in pieces is served whole, and the connection closes behind its answer.
#[test]
fn test_a_request_in_pieces_is_served_whole() -> TestResult {
    Runtime::new()?.block_on(async {
        let mut stream = connected(HttpLimits::new(8, Duration::from_secs(5))).await?;
        for piece in ["POST / HTTP/1.1\r\nContent-", "Length: 2\r\n\r\na", "b"] {
            let BufResult(written, _) = stream.write_all(piece.as_bytes().to_vec()).await;
            written?;
            compio::time::sleep(Duration::from_millis(5)).await;
        }
        let text = answer(&mut stream).await?;
        assert!(text.starts_with("HTTP/1.1 200 OK"), "{text}");
        assert!(text.ends_with("fine"), "{text}");
        Ok(())
    })
}

/// A peer that stops mid-request gets a 400, and one that says nothing past the time gets a 408.
#[test]
fn test_an_unfinished_request_is_refused() -> TestResult {
    Runtime::new()?.block_on(async {
        let mut stopping = connected(HttpLimits::new(8, Duration::from_secs(5))).await?;
        let BufResult(written, _) = stopping.write_all(b"GET /hea".to_vec()).await;
        written?;
        stopping.shutdown().await?;
        assert!(answer(&mut stopping).await?.starts_with("HTTP/1.1 400"));
        let mut silent = connected(HttpLimits::new(8, Duration::from_millis(20))).await?;
        assert!(answer(&mut silent).await?.starts_with("HTTP/1.1 408"));
        Ok(())
    })
}

/// A body past the limit is refused from its head alone.
#[test]
fn test_an_oversized_body_is_refused_unread() -> TestResult {
    Runtime::new()?.block_on(async {
        let mut stream = connected(HttpLimits::new(8, Duration::from_secs(5))).await?;
        let BufResult(written, _) = stream
            .write_all(b"POST / HTTP/1.1\r\nContent-Length: 9\r\n\r\n".to_vec())
            .await;
        written?;
        assert!(answer(&mut stream).await?.starts_with("HTTP/1.1 413"));
        Ok(())
    })
}
