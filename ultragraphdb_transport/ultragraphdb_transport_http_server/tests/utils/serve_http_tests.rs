/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compio::BufResult;
use compio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use compio::net::{TcpListener, TcpStream};
use compio::runtime::{Runtime, spawn};
use ultragraphdb_tool_bdd::{TestError, TestResult};
use ultragraphdb_transport_http_server::{
    HttpLimits, HttpRequest, HttpResponse, HttpService, serve_http,
};

/// The bytes one read of a reply asks for.
const READ_BYTES: usize = 4 * 1024;

/// A piece of a body a client goes on sending: more than the one read a server takes of a refused request leaves unread.
const PIECE: usize = 64 * 1024;

/// How long a client waits before its next piece: long enough for a close to have reached it as a reset.
const PAUSE: Duration = Duration::from_millis(5);

/// How many pauses a client writes through, waiting for a close: 5 s, far past any time the tests name.
const CLOSE_TURNS: u32 = 1_000;

#[derive(Clone, Copy)]
struct Fine;

impl HttpService for Fine {
    async fn call(&self, _request: HttpRequest) -> HttpResponse {
        HttpResponse::new(200).with_body("fine")
    }
}

/// Answers as `Fine` does, counting the calls.
#[derive(Clone)]
struct Counted(Rc<Cell<u32>>);

impl HttpService for Counted {
    async fn call(&self, _request: HttpRequest) -> HttpResponse {
        self.0.set(self.0.get() + 1);
        HttpResponse::new(200)
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

/// A connection to a server of `limits` that counts what `Counted` is asked: the connection and the count.
async fn counted(limits: HttpLimits) -> TestResult<(TcpStream, Rc<Cell<u32>>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let calls = Rc::new(Cell::new(0));
    let service = Counted(calls.clone());
    spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            serve_http(stream, &limits, &service).await;
        }
    })
    .detach();
    Ok((TcpStream::connect(address).await?, calls))
}

/// The first bytes of the reply to what `stream` has sent, whatever it sends next.
async fn first_of_reply(stream: &mut TcpStream) -> TestResult<String> {
    let BufResult(read, bytes) = stream.read(Vec::with_capacity(READ_BYTES)).await;
    let _count: usize = read?;
    Ok(String::from_utf8(bytes)?)
}

/// `pieces` writes of a piece each, a pause before each, each naming itself when it fails.
async fn write_on(stream: &mut TcpStream, pieces: u32) -> TestResult {
    for piece in 0..pieces {
        compio::time::sleep(PAUSE).await;
        let BufResult(written, _) = stream.write_all(vec![b'x'; PIECE]).await;
        written
            .map_err(|error| TestError::new(format!("write {piece} after a refusal: {error}")))?;
    }
    Ok(())
}

/// Whatever a request is refused for before its body is whole, a client still sending reads the refusal, every write
/// taken, and then the connection's end with no reset; the service hears of none of it.
#[test]
fn test_a_refusal_reaches_a_client_still_sending() -> TestResult {
    let head_past = format!("GET /{} HTTP/1.1\r\n", "a".repeat(HttpLimits::HEAD_MAX));
    let refused = [
        (
            "POST / HTTP/1.1\r\nContent-Length: 9\r\n\r\n".to_string(),
            8,
            "413",
        ),
        (
            "POST /health HTTP/1.1\r\nContent-Length: 1\r\n\r\n".to_string(),
            0,
            "413",
        ),
        (
            "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n".to_string(),
            8,
            "501",
        ),
        ("POST / HTP\r\n\r\n".to_string(), 8, "400"),
        (head_past, 8, "431"),
    ];
    Runtime::new()?.block_on(async {
        for (request, body, status) in refused {
            let (mut stream, calls) =
                counted(HttpLimits::new(body, Duration::from_secs(5))).await?;
            let mut sent = request.into_bytes();
            sent.extend_from_slice(&[b'x'; PIECE]);
            let BufResult(written, _) = stream.write_all(sent).await;
            written?;
            let reply = first_of_reply(&mut stream).await?;
            assert!(reply.starts_with(&format!("HTTP/1.1 {status}")), "{reply}");
            write_on(&mut stream, 4).await?;
            stream.shutdown().await?;
            let _end: String = answer(&mut stream).await?;
            assert_eq!(calls.get(), 0, "{status}");
        }
        Ok(())
    })
}

/// A client too slow for the request's time reads its 408 and may write on, as one refused for its size may.
#[test]
fn test_a_slow_client_reads_its_refusal_while_still_sending() -> TestResult {
    Runtime::new()?.block_on(async {
        let mut stream = connected(HttpLimits::new(8, Duration::from_millis(500))).await?;
        let BufResult(written, _) = stream.write_all(b"POST / HT".to_vec()).await;
        written?;
        assert!(
            first_of_reply(&mut stream)
                .await?
                .starts_with("HTTP/1.1 408")
        );
        write_on(&mut stream, 4).await?;
        stream.shutdown().await?;
        let _end: String = answer(&mut stream).await?;
        Ok(())
    })
}

/// A client that holds its connection open and says nothing more has it closed at the request's time.
#[test]
fn test_a_client_holding_a_refused_connection_loses_it_at_the_time() -> TestResult {
    Runtime::new()?.block_on(async {
        let mut stream = connected(HttpLimits::new(8, Duration::from_millis(200))).await?;
        let BufResult(written, _) = stream
            .write_all(b"POST / HTTP/1.1\r\nContent-Length: 9\r\n\r\n".to_vec())
            .await;
        written?;
        assert!(
            first_of_reply(&mut stream)
                .await?
                .starts_with("HTTP/1.1 413")
        );
        write_on(&mut stream, 2).await?;
        until_closed(&mut stream).await
    })
}

/// A client sending without end has its connection closed at the most a drain reads, its time far off.
#[test]
fn test_a_client_sending_without_end_loses_its_connection_at_the_bytes() -> TestResult {
    Runtime::new()?.block_on(async {
        let mut stream = connected(HttpLimits::new(8, Duration::from_secs(60))).await?;
        let BufResult(written, _) = stream
            .write_all(b"POST / HTTP/1.1\r\nContent-Length: 9\r\n\r\n".to_vec())
            .await;
        written?;
        assert!(
            first_of_reply(&mut stream)
                .await?
                .starts_with("HTTP/1.1 413")
        );
        let mut buffer = vec![b'x'; 1 << 20];
        let mut sent = 0;
        // Past the most a drain reads, and whatever the two kernels' buffers hold beside it.
        while sent < 4 * HttpLimits::DRAIN_MAX {
            let BufResult(taken, returned) = stream.write_all(buffer).await;
            buffer = returned;
            if taken.is_err() {
                // The write that failed may have taken part of its buffer.
                let most = sent + buffer.len();
                assert!(most >= HttpLimits::DRAIN_MAX, "closed by {most} bytes");
                return Ok(());
            }
            sent += buffer.len();
        }
        Err(TestError::new(format!(
            "the connection still takes writes after {sent} bytes"
        )))
    })
}

/// Writes through [`CLOSE_TURNS`] pauses until a write fails, the server's close having reached this socket as a reset.
async fn until_closed(stream: &mut TcpStream) -> TestResult {
    for _ in 0..CLOSE_TURNS {
        let BufResult(written, _) = stream.write_all(vec![b'x'; 1024]).await;
        if written.is_err() {
            return Ok(());
        }
        compio::time::sleep(PAUSE).await;
    }
    Err(TestError::new(format!(
        "the server's close, through {CLOSE_TURNS} writes"
    )))
}
