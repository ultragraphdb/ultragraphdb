/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use compio::BufResult;
use compio::buf::{IoBuf, IoBufMut};
use compio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use compio::net::{TcpListener, TcpStream};
use compio::runtime::{Runtime, spawn};
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_transport_compio::{CompioTlsStream, Listener};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transport_http_server::{
    HttpLimits, HttpRequest, HttpResponse, HttpService, serve_http, serve_listener,
};
use ultragraphdb_transport_tls::TestPki;

/// The bytes one read of a reply asks for.
const READ_BYTES: usize = 4 * 1024;

/// A piece of a body a client goes on sending: more than the one read a server takes of a refused request leaves unread.
const PIECE: usize = 64 * 1024;

/// How long a client waits before its next piece: long enough for a close to have reached it as a reset.
const PAUSE: Duration = Duration::from_millis(5);

/// How many pauses a client writes through, waiting for a close: 5 s, far past any time the tests name.
const CLOSE_TURNS: u32 = 1_000;

/// How long a client waits on anything a server owes it at once: far past what a loopback takes, so a server that owes it
/// and never sends fails the test rather than holding it.
const WAIT: Duration = Duration::from_secs(5);

/// `future`'s output, or a failure naming `what` when it takes past [`WAIT`].
async fn within<T>(what: &str, future: impl Future<Output = T>) -> TestResult<T> {
    compio::time::timeout(WAIT, future)
        .await
        .map_err(|_| TestError::new(format!("{what} within {WAIT:?}")))
}

#[derive(Clone, Copy)]
struct Fine;

impl HttpService for Fine {
    async fn call(&self, _request: HttpRequest) -> HttpResponse {
        HttpResponse::new(200).with_body("fine")
    }
}

/// Answers as `Fine` does, counting the calls.
#[derive(Clone)]
struct Counted(Arc<AtomicU32>);

impl HttpService for Counted {
    async fn call(&self, _request: HttpRequest) -> HttpResponse {
        let _before = self.0.fetch_add(1, Ordering::Relaxed);
        HttpResponse::new(200)
    }
}

/// Serves the one connection `listener` takes by `service` within `limits`, on a thread and a runtime of its own, so a
/// server that never yields cannot hold the test's own bounded waits.
fn serve_apart<H: HttpService + Send + 'static>(
    listener: std::net::TcpListener,
    limits: HttpLimits,
    service: H,
) {
    compio::runtime::spawn_blocking(move || {
        let Ok(runtime) = Runtime::new() else {
            return;
        };
        runtime.block_on(async move {
            let Ok(listener) = TcpListener::from_std(listener) else {
                return;
            };
            if let Ok((stream, _)) = listener.accept().await {
                serve_http(stream, &limits, &service).await;
            }
        });
    })
    .detach();
}

/// A connection to a server of `limits`.
async fn connected(limits: HttpLimits) -> TestResult<TcpStream> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    serve_apart(listener, limits, Fine);
    Ok(TcpStream::connect(address).await?)
}

async fn answer(stream: &mut TcpStream) -> TestResult<String> {
    let BufResult(read, bytes) = within("the answer", stream.read_to_end(Vec::new())).await?;
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
async fn counted(limits: HttpLimits) -> TestResult<(TcpStream, Arc<AtomicU32>)> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let calls = Arc::new(AtomicU32::new(0));
    serve_apart(listener, limits, Counted(Arc::clone(&calls)));
    Ok((TcpStream::connect(address).await?, calls))
}

/// The first bytes of the reply to what `stream` has sent, whatever it sends next.
async fn first_of_reply(stream: &mut TcpStream) -> TestResult<String> {
    let BufResult(read, bytes) =
        within("a reply", stream.read(Vec::with_capacity(READ_BYTES))).await?;
    let _count: usize = read?;
    Ok(String::from_utf8(bytes)?)
}

/// `pieces` writes of a piece each, a pause before each, each naming itself when it fails.
async fn write_on(stream: &mut TcpStream, pieces: u32) -> TestResult {
    for piece in 0..pieces {
        compio::time::sleep(PAUSE).await;
        let BufResult(written, _) = within("a write", stream.write_all(vec![b'x'; PIECE])).await?;
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
            assert_eq!(calls.load(Ordering::Relaxed), 0, "{status}");
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
            let BufResult(taken, returned) = within("a write", stream.write_all(buffer)).await?;
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
        let BufResult(written, _) = within("a write", stream.write_all(vec![b'x'; 1024])).await?;
        if written.is_err() {
            return Ok(());
        }
        compio::time::sleep(PAUSE).await;
    }
    Err(TestError::new(format!(
        "the server's close, through {CLOSE_TURNS} writes"
    )))
}

/// The most one read of a [`Peer`] gives: a divisor of [`HttpLimits::DRAIN_MAX`], below what a server's read asks for.
const PEER_READ: usize = 4 * 1024;

/// A mistaken upload a refused peer goes on sending: 4 MiB, eight times the 512 KiB the config service takes, inside
/// the 16 MiB a drain reads.
const UPLOAD: usize = 4 * 1024 * 1024;

/// A peer in memory: `head`, then `body` bytes of `x` (without end when `None`), each read giving at most `step` bytes and
/// never crossing from the head into the body; then its end. It counts the body bytes the server read, notes a read
/// after its end, and keeps what the server wrote, or refuses every write when `write_fails`.
struct Peer {
    head: Vec<u8>,
    head_read: usize,
    body: Option<usize>,
    step: usize,
    body_read: usize,
    ended: bool,
    read_past_end: bool,
    written: Vec<u8>,
    write_fails: bool,
}

impl Peer {
    fn new(head: &[u8], body: Option<usize>, step: usize) -> Self {
        Self {
            head: head.to_vec(),
            head_read: 0,
            body,
            step,
            body_read: 0,
            ended: false,
            read_past_end: false,
            written: Vec::new(),
            write_fails: false,
        }
    }

    /// The next read's bytes, or `None` at the end.
    fn next_chunk(&mut self, room: usize) -> Option<Vec<u8>> {
        let room = room.min(self.step);
        if self.head_read < self.head.len() {
            let end = self.head.len().min(self.head_read + room);
            let chunk = self
                .head
                .get(self.head_read..end)
                .unwrap_or_default()
                .to_vec();
            self.head_read = end;
            return Some(chunk);
        }
        let left = self
            .body
            .map_or(room, |body| body.saturating_sub(self.body_read));
        if left == 0 {
            return None;
        }
        let taken = room.min(left);
        self.body_read += taken;
        Some(vec![b'x'; taken])
    }
}

impl AsyncRead for Peer {
    async fn read<B: IoBufMut>(&mut self, mut buf: B) -> BufResult<usize, B> {
        let room = buf.buf_capacity().saturating_sub(buf.buf_len());
        match self.next_chunk(room) {
            Some(chunk) => {
                let mut source: &[u8] = &chunk;
                source.read(buf).await
            }
            None if self.ended => {
                self.read_past_end = true;
                BufResult(Err(std::io::Error::other("a read past the end")), buf)
            }
            None => {
                self.ended = true;
                BufResult(Ok(0), buf)
            }
        }
    }
}

impl AsyncWrite for Peer {
    async fn write<T: IoBuf>(&mut self, buf: T) -> BufResult<usize, T> {
        if self.write_fails {
            return BufResult(Err(std::io::ErrorKind::BrokenPipe.into()), buf);
        }
        self.written.extend_from_slice(buf.as_init());
        let len = buf.as_init().len();
        BufResult(Ok(len), buf)
    }

    async fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    async fn shutdown(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// `peer` served by `Fine` within `limits`, then what it was written.
fn served(mut peer: Peer, limits: HttpLimits) -> TestResult<(Peer, String)> {
    Runtime::new()?.block_on(async {
        serve_http(&mut peer, &limits, &Fine).await;
        let written = String::from_utf8(peer.written.clone())?;
        Ok((peer, written))
    })
}

/// A refused peer sending a mistaken upload reads its refusal, and the server reads the upload to its end before closing,
/// and not past it.
#[test]
fn test_a_refused_upload_is_drained_to_its_end() -> TestResult {
    let peer = Peer::new(
        b"POST / HTTP/1.1\r\nContent-Length: 9\r\n\r\n",
        Some(UPLOAD),
        PEER_READ,
    );
    let (peer, written) = served(peer, HttpLimits::new(8, Duration::from_secs(60)))?;
    assert!(written.starts_with("HTTP/1.1 413"), "{written}");
    assert_eq!(peer.body_read, UPLOAD);
    assert!(peer.ended && !peer.read_past_end);
    Ok(())
}

/// A refused peer sending without end has the server stop reading once `DRAIN_MAX` bytes have passed: at the read that
/// reaches it, and not one read later.
#[test]
fn test_a_drain_stops_at_its_bytes() -> TestResult {
    let peer = Peer::new(
        b"POST / HTTP/1.1\r\nContent-Length: 9\r\n\r\n",
        None,
        PEER_READ,
    );
    let (peer, written) = served(peer, HttpLimits::new(8, Duration::from_secs(60)))?;
    assert!(written.starts_with("HTTP/1.1 413"), "{written}");
    assert!(
        (HttpLimits::DRAIN_MAX..HttpLimits::DRAIN_MAX + PEER_READ).contains(&peer.body_read),
        "{} bytes read",
        peer.body_read
    );
    Ok(())
}

/// A refused peer whose connection takes no write is left: nothing reaches it, and its upload is not drained.
#[test]
fn test_a_peer_taking_no_write_is_left_undrained() -> TestResult {
    let mut peer = Peer::new(
        b"POST / HTTP/1.1\r\nContent-Length: 9\r\n\r\n",
        Some(UPLOAD),
        PEER_READ,
    );
    peer.write_fails = true;
    let (peer, written) = served(peer, HttpLimits::new(8, WAIT))?;
    assert!(written.is_empty(), "{written}");
    assert_eq!(peer.body_read, 0);
    Ok(())
}

/// A served request is answered and closed: what follows it is not read, and the body taken is the declared one alone.
#[test]
fn test_a_served_request_reads_nothing_past_its_body() -> TestResult {
    let peer = Peer::new(
        b"POST / HTTP/1.1\r\nContent-Length: 4\r\n\r\nbodyEXTRA",
        Some(PEER_READ),
        PEER_READ,
    );
    let mut peer = peer;
    let taken = Rc::new(Cell::new(0));
    let service = Taken(taken.clone());
    Runtime::new()?.block_on(serve_http(&mut peer, &HttpLimits::new(8, WAIT), &service));
    assert_eq!(taken.get(), "body".len());
    assert_eq!(peer.body_read, 0);
    assert!(String::from_utf8(peer.written)?.starts_with("HTTP/1.1 200"));
    Ok(())
}

/// Answers 200, noting how long the body it took is.
#[derive(Clone)]
struct Taken(Rc<Cell<usize>>);

impl HttpService for Taken {
    async fn call(&self, request: HttpRequest) -> HttpResponse {
        self.0.set(request.into_body().len());
        HttpResponse::new(200)
    }
}

/// A request arriving a byte at a time is served whole, its head near the most a head takes and its body at the most the
/// limits take; and a server taking no body serves a request that has none.
#[test]
fn test_a_request_a_byte_at_a_time_is_served() -> TestResult {
    let body = 1024;
    let target = "a".repeat(HttpLimits::HEAD_MAX - 64);
    let head = format!("POST /{target} HTTP/1.1\r\nContent-Length: {body}\r\n\r\n");
    let (_, written) = served(
        Peer::new(head.as_bytes(), Some(body), 1),
        HttpLimits::new(body, WAIT),
    )?;
    assert!(written.starts_with("HTTP/1.1 200"), "{written}");
    let (_, bare) = served(
        Peer::new(b"GET /health HTTP/1.1\r\n\r\n", Some(0), PEER_READ),
        HttpLimits::new(0, WAIT),
    )?;
    assert!(bare.starts_with("HTTP/1.1 200"), "{bare}");
    Ok(())
}

/// A listener in clear text answers each connection it accepts, one after another.
#[test]
fn test_a_listener_answers_every_connection() -> TestResult {
    Runtime::new()?.block_on(async {
        let listener = Listener::bind(&Address::any_port("127.0.0.1")?).await?;
        let address = listener.local_address()?;
        let target = address.host_port().context("a TCP address")?.to_string();
        spawn(serve_listener(
            listener,
            None,
            HttpLimits::new(0, WAIT),
            Fine,
        ))
        .detach();
        for _ in 0..2 {
            let mut stream = within("a connection", TcpStream::connect(&target)).await??;
            let BufResult(written, _) = stream
                .write_all(b"GET /health HTTP/1.1\r\n\r\n".to_vec())
                .await;
            written?;
            let text = answer(&mut stream).await?;
            assert!(text.starts_with("HTTP/1.1 200"), "{text}");
        }
        Ok(())
    })
}

/// How long a TLS listener's tests give a connection: a loopback handshake takes far less, and a silent client is closed
/// well within [`WAIT`].
const TLS_TIME: Duration = Duration::from_millis(500);

/// A listener over TLS answers a verified client; a client whose handshake fails and one that never finishes its
/// handshake cost their own connections alone, the silent one closed at the connection's time.
#[test]
fn test_a_tls_listener_answers_past_failed_handshakes() -> TestResult {
    Runtime::new()?.block_on(async {
        let pki = TestPki::new()?;
        let listener = Listener::bind(&Address::any_port("127.0.0.1")?).await?;
        let address = listener.local_address()?;
        let target = address.host_port().context("a TCP address")?.to_string();
        spawn(serve_listener(
            listener,
            Some(pki.acceptor()?),
            HttpLimits::new(0, TLS_TIME),
            Fine,
        ))
        .detach();
        let mut clear = within("a connection", TcpStream::connect(&target)).await??;
        let BufResult(written, _) = clear
            .write_all(b"GET /health HTTP/1.1\r\n\r\n".to_vec())
            .await;
        written?;
        let refused = answer(&mut clear).await;
        assert!(
            !refused
                .as_ref()
                .is_ok_and(|text| text.starts_with("HTTP/1.1")),
            "{refused:?}"
        );
        let mut silent = within("a connection", TcpStream::connect(&target)).await??;
        let BufResult(read, _) =
            within("the silent close", silent.read(Vec::with_capacity(1))).await?;
        assert_eq!(read.unwrap_or_default(), 0, "the silent client is closed");
        let carrier = within("a connection", TcpStream::connect(&target)).await??;
        let mut tls = within(
            "a handshake",
            CompioTlsStream::connect(&pki.connector()?, carrier, "localhost"),
        )
        .await??;
        let BufResult(sent, _) = tls
            .write_all(b"GET /health HTTP/1.1\r\n\r\n".to_vec())
            .await;
        sent?;
        let BufResult(answered, bytes) = within("the answer", tls.read_to_end(Vec::new())).await?;
        let _count: usize = answered?;
        let text = String::from_utf8(bytes)?;
        assert!(text.starts_with("HTTP/1.1 200"), "{text}");
        Ok(())
    })
}

/// How long a listener's held connections may stay silent: past any test, so each holds its place until the test lets go.
const HOLD: Duration = Duration::from_secs(60);

/// How long a connection past the most is watched for a call that must not come: a loopback request is served in far less.
const UNSERVED: Duration = Duration::from_millis(200);

/// A listener of `limits` serving by `Counted` beside the test: where it listens, and its count of calls.
async fn counting_listener(limits: HttpLimits) -> TestResult<(String, Arc<AtomicU32>)> {
    let listener = Listener::bind(&Address::any_port("127.0.0.1")?).await?;
    let target = listener
        .local_address()?
        .host_port()
        .context("a TCP address")?
        .to_string();
    let calls = Arc::new(AtomicU32::new(0));
    spawn(serve_listener(
        listener,
        None,
        limits,
        Counted(Arc::clone(&calls)),
    ))
    .detach();
    Ok((target, calls))
}

/// `count` connections to `target` that send nothing, each holding its place at the listener.
async fn held(target: &str, count: usize) -> TestResult<Vec<TcpStream>> {
    let mut streams = Vec::with_capacity(count);
    for _ in 0..count {
        streams.push(within("a connection", TcpStream::connect(target)).await??);
    }
    Ok(streams)
}

/// A connection to `target` that has sent its request.
async fn asking(target: &str) -> TestResult<TcpStream> {
    let mut stream = within("a connection", TcpStream::connect(target)).await??;
    let BufResult(written, _) = stream
        .write_all(b"GET /health HTTP/1.1\r\n\r\n".to_vec())
        .await;
    written?;
    Ok(stream)
}

/// The limits of a listener of two connections at once, the fewest that tell the last place from the first.
fn two_at_once() -> TestResult<HttpLimits> {
    Ok(HttpLimits::new(0, HOLD).with_connections(std::num::NonZeroUsize::new(2).context("two")?))
}

/// A listener serves its most connections at once: with all but one held silent, the last one's request is answered.
#[test]
fn test_a_listener_serves_up_to_its_most_connections() -> TestResult {
    Runtime::new()?.block_on(async {
        let limits = two_at_once()?;
        let (target, calls) = counting_listener(limits).await?;
        let _held = held(&target, limits.connections().get() - 1).await?;
        let mut last = asking(&target).await?;
        let text = answer(&mut last).await?;
        assert!(text.starts_with("HTTP/1.1 200"), "{text}");
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        Ok(())
    })
}

/// A connection past a listener's most waits, unserved, until one of the connections it holds ends, and is then answered.
#[test]
fn test_a_connection_past_the_most_waits_for_one_to_end() -> TestResult {
    Runtime::new()?.block_on(async {
        let limits = two_at_once()?;
        let (target, calls) = counting_listener(limits).await?;
        let mut holding = held(&target, limits.connections().get()).await?;
        let mut past = asking(&target).await?;
        compio::time::sleep(UNSERVED).await;
        assert_eq!(calls.load(Ordering::Relaxed), 0, "a call past the most");
        drop(holding.pop().context("a held connection")?);
        let text = answer(&mut past).await?;
        assert!(text.starts_with("HTTP/1.1 200"), "{text}");
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        Ok(())
    })
}
