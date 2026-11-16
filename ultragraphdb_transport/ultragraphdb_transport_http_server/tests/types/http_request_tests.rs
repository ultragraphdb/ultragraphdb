/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use compio::BufResult;
use compio::io::{AsyncReadExt, AsyncWriteExt};
use compio::net::{TcpListener, TcpStream};
use compio::runtime::{Runtime, spawn};
use ultragraphdb_tool_bdd::{TestResult, hides};
use ultragraphdb_transport_http_server::{
    HttpLimits, HttpRequest, HttpResponse, HttpService, serve_http,
};

/// Echoes what it read of the request in its body.
#[derive(Clone, Copy)]
struct Reads;

impl HttpService for Reads {
    async fn call(&self, request: HttpRequest) -> HttpResponse {
        let line = format!(
            "{} {} base={:?} other={:?} key={:?} body={}",
            request.method(),
            request.path(),
            request.query("base"),
            request.query("other"),
            request.bearer(),
            String::from_utf8_lossy(request.body()),
        );
        let moved = request.into_body();
        HttpResponse::new(200).with_body(format!("{line} moved={}", moved.len()))
    }
}

/// A request's path, query, key and body read apart, and its body taken alone.
#[test]
fn test_a_request_reads_its_parts() -> TestResult {
    Runtime::new()?.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let limits = HttpLimits::new(64, Duration::from_secs(5));
        spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                serve_http(stream, &limits, &Reads).await;
            }
        })
        .detach();
        let mut stream = TcpStream::connect(address).await?;
        let sent = b"POST /a/b?base=3&x=1 HTTP/1.1\r\nAuthorization: Bearer k\r\nContent-Length: 4\r\n\r\nbody".to_vec();
        let BufResult(written, _) = stream.write_all(sent).await;
        written?;
        let BufResult(read, answer) = stream.read_to_end(Vec::new()).await;
        let _count: usize = read?;
        let text = String::from_utf8(answer)?;
        assert!(
            text.ends_with("POST /a/b base=Some(\"3\") other=None key=Some(\"k\") body=body moved=4"),
            "{text}"
        );
        Ok(())
    })
}

/// Answers with the request's `Debug`, compact and pretty.
#[derive(Clone, Copy)]
struct Prints;

impl HttpService for Prints {
    async fn call(&self, request: HttpRequest) -> HttpResponse {
        HttpResponse::new(200).with_body(format!("{request:?}\n{request:#?}"))
    }
}

/// A request's `Debug` holds its target and body and never the key it presents.
#[test]
fn test_a_request_never_prints_its_key() -> TestResult {
    let key = "a bearer key that never prints, 32+ bytes";
    Runtime::new()?.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let limits = HttpLimits::new(64, Duration::from_secs(5));
        spawn(async move {
            if let Ok((stream, _)) = listener.accept().await {
                serve_http(stream, &limits, &Prints).await;
            }
        })
        .detach();
        let mut stream = TcpStream::connect(address).await?;
        let sent = format!(
            "POST /a/b HTTP/1.1\r\nAuthorization: Bearer {key}\r\nContent-Length: 4\r\n\r\nbody"
        )
        .into_bytes();
        let BufResult(written, _) = stream.write_all(sent).await;
        written?;
        let BufResult(read, answer) = stream.read_to_end(Vec::new()).await;
        let _count: usize = read?;
        let text = String::from_utf8(answer)?;
        hides(&text, key.as_bytes())?;
        assert!(text.contains("target: \"/a/b\""), "{text}");
        assert!(text.contains(&format!("body: {:?}", b"body")), "{text}");
        Ok(())
    })
}
