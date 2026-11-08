/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;
use std::io::ErrorKind;
use std::rc::Rc;

use compio::BufResult;
use compio::buf::{IoBuf, IoBufMut};
use compio::io::{AsyncRead, AsyncWrite};
use rustls::ProtocolVersion;
use ultragraphdb_protocol_transport::{Address, BufferPool, Framer, NetworkFaults, SimNetwork};
use ultragraphdb_protocol_transport_compio::{
    Carrier, CompioSimStream, CompioTlsStream, Connection, Listener,
};
use ultragraphdb_protocol_wire::{Hello, MAX_FRAME_SIZE, Welcome};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transport_tls::{TestPki, TlsError, TlsErrorEnum};

use crate::utils::bytes::{read_once, receive, send};
use crate::utils::ready::ready;
use crate::utils::sim::connect;
use crate::utils::socket::SocketDir;

const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

/// A server's and a client's TLS stream over carriers of `address`, their handshake completed.
async fn tls_pair(
    pki: &TestPki,
    address: &Address,
) -> TestResult<(CompioTlsStream<Carrier>, CompioTlsStream<Carrier>)> {
    let listener = Listener::bind(address).await?;
    let bound = listener.local_address()?;
    let (accepted, dialled) =
        futures::future::join(listener.accept(), Carrier::connect(&bound)).await;
    let (acceptor, connector) = (pki.acceptor()?, pki.connector()?);
    let (server, client) = futures::future::join(
        CompioTlsStream::accept(&acceptor, accepted?),
        CompioTlsStream::connect(&connector, dialled?, "localhost"),
    )
    .await;
    Ok((server?, client?))
}

#[test]
fn test_a_line_crosses_tls_over_tcp_and_over_a_unix_socket() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let dir = SocketDir::new()?;
        let pki = TestPki::new()?;
        for address in [Address::any_port("127.0.0.1")?, dir.address("s.sock")?] {
            let (mut server, mut client) = tls_pair(&pki, &address).await?;
            assert_eq!(
                client.session().protocol_version(),
                Some(ProtocolVersion::TLSv1_3)
            );
            assert_eq!(
                server.session().protocol_version(),
                Some(ProtocolVersion::TLSv1_3)
            );
            assert_eq!(
                (client.session().host(), server.session().host()),
                ("localhost", "")
            );
            send(&mut client, b"ping\n").await?;
            let line = receive(&mut server, 5).await?;
            assert_eq!(line, b"ping\n", "{address}");
            send(&mut server, &line).await?;
            assert_eq!(receive(&mut client, 5).await?, b"ping\n", "{address}");
            // The client's close_notify reads as the end on the server.
            client.shutdown().await.context("a shutdown")?;
            assert_eq!(read_once(&mut server).await?, 0, "{address}");
            assert!(matches!(
                server.get_ref(),
                Carrier::Tcp(_) | Carrier::Unix(_)
            ));
        }
        Ok::<(), TestError>(())
    })
}

#[test]
fn test_a_connection_runs_its_handshake_inside_tls_over_a_unix_socket() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let dir = SocketDir::new()?;
        let pki = TestPki::new()?;
        let (server, client) = tls_pair(&pki, &dir.address("s.sock")?).await?;
        let mut server = Connection::new(server, Framer::from_client(CLUSTER, MAX_FRAME_SIZE));
        let mut client = Connection::new(client, Framer::new(CLUSTER, MAX_FRAME_SIZE));
        let pool = BufferPool::new(4, MAX_FRAME_SIZE);
        let welcome = Welcome::new(1, 0, "s")
            .with_max_streams(8)
            .with_initial_credit(2);
        let hello = Hello::new(1, 1, 0, MAX_FRAME_SIZE as u32, "c");
        let (client_side, server_side) =
            futures::future::join(client.hello(&pool, &hello), server.welcome(&pool, &welcome))
                .await;
        assert_eq!(client_side?, welcome);
        assert_eq!(server_side?, welcome);
        Ok::<(), TestError>(())
    })
}

#[test]
fn test_a_server_of_an_untrusted_ca_fails_both_handshakes() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let (pki, other) = (TestPki::new()?, TestPki::new()?);
        let listener = Listener::bind(&Address::any_port("127.0.0.1")?).await?;
        let bound = listener.local_address()?;
        let (accepted, dialled) =
            futures::future::join(listener.accept(), Carrier::connect(&bound)).await;
        let (acceptor, connector) = (pki.acceptor()?, other.connector()?);
        let (server, client) = futures::future::join(
            CompioTlsStream::accept(&acceptor, accepted?),
            CompioTlsStream::connect(&connector, dialled?, "localhost"),
        )
        .await;
        let Err(error) = client else {
            return Err(TestError::new("another CA's certificate verified"));
        };
        assert!(
            matches!(&error.0, TlsErrorEnum::HandshakeFailed { host, reason }
                if host == "localhost" && reason.contains("UnknownIssuer")),
            "{error:?}"
        );
        // The server took the client's alert as a failed handshake, its host empty.
        let Err(refused) = server else {
            return Err(TestError::new("the server completed a refused handshake"));
        };
        assert!(
            matches!(&refused.0, TlsErrorEnum::HandshakeFailed { host, reason }
                if host.is_empty() && reason.contains("UnknownCA")),
            "{refused:?}"
        );
        Ok::<(), TestError>(())
    })
}

#[test]
fn test_a_client_that_names_no_server_is_refused_before_dialling_bytes() -> TestResult {
    compio::runtime::Runtime::new()?.block_on(async {
        let pki = TestPki::new()?;
        let listener = Listener::bind(&Address::any_port("127.0.0.1")?).await?;
        let dialled = Carrier::connect(&listener.local_address()?).await?;
        let Err(error) = CompioTlsStream::connect(&pki.connector()?, dialled, "not a host").await
        else {
            return Err(TestError::new("\"not a host\" was taken as a server name"));
        };
        assert_eq!(
            error.0,
            TlsErrorEnum::InvalidServerName("not a host".to_string())
        );
        Ok::<(), TestError>(())
    })
}

/// What a [`Faulty`] stream does instead of its next reads and writes.
#[derive(Debug, Default)]
struct Faults {
    interrupt_next_read: Cell<bool>,
    fail_reads: Cell<bool>,
    fail_writes: Cell<bool>,
}

/// A simulated stream of the tests' own, its faults switched by the test through `faults`: a read interrupted once, as
/// a signal interrupts a system call, reads failing with `ConnectionReset`, writes and flushes with `BrokenPipe`.
#[derive(Debug)]
struct Faulty {
    inner: CompioSimStream,
    faults: Rc<Faults>,
}

impl AsyncRead for Faulty {
    async fn read<B: IoBufMut>(&mut self, buf: B) -> BufResult<usize, B> {
        if self.faults.interrupt_next_read.replace(false) {
            return BufResult(Err(ErrorKind::Interrupted.into()), buf);
        }
        if self.faults.fail_reads.get() {
            return BufResult(Err(ErrorKind::ConnectionReset.into()), buf);
        }
        self.inner.read(buf).await
    }
}

impl AsyncWrite for Faulty {
    async fn write<T: IoBuf>(&mut self, buf: T) -> BufResult<usize, T> {
        if self.faults.fail_writes.get() {
            return BufResult(Err(ErrorKind::BrokenPipe.into()), buf);
        }
        self.inner.write(buf).await
    }

    async fn flush(&mut self) -> std::io::Result<()> {
        if self.faults.fail_writes.get() {
            return Err(ErrorKind::BrokenPipe.into());
        }
        self.inner.flush().await
    }

    async fn shutdown(&mut self) -> std::io::Result<()> {
        self.inner.shutdown().await
    }
}

/// A simulated connection's two ends as [`Faulty`] streams, each with the faults the test switches.
fn faulty_ends(seed: u64) -> ((Faulty, Rc<Faults>), (Faulty, Rc<Faults>)) {
    let network = SimNetwork::new(seed, NetworkFaults::none());
    let (client, server) = connect(&network);
    let end = |inner| {
        let faults = Rc::new(Faults::default());
        (
            Faulty {
                inner,
                faults: Rc::clone(&faults),
            },
            faults,
        )
    };
    (end(client), end(server))
}

/// A server's and a client's TLS stream over [`Faulty`] ends, their handshake completed, and the client's faults and
/// the server's.
type FaultyPair = (
    CompioTlsStream<Faulty>,
    CompioTlsStream<Faulty>,
    Rc<Faults>,
    Rc<Faults>,
);

fn faulty_pair(pki: &TestPki) -> TestResult<FaultyPair> {
    let ((client, client_faults), (server, server_faults)) = faulty_ends(1);
    let (acceptor, connector) = (pki.acceptor()?, pki.connector()?);
    let (server, client) = ready(futures::future::join(
        CompioTlsStream::accept(&acceptor, server),
        CompioTlsStream::connect(&connector, client, "localhost"),
    ));
    Ok((server?, client?, client_faults, server_faults))
}

/// The error the `io` error of `kind` reads as, a failed handshake's reason.
fn reason(kind: ErrorKind) -> String {
    std::io::Error::from(kind).to_string()
}

#[test]
fn test_a_stream_closed_by_its_peer_fails_the_handshake_naming_the_host() -> TestResult {
    let pki = TestPki::new()?;
    let ((client, _), (server, _)) = faulty_ends(2);
    server.inner.sim().shutdown();
    let Err(error) = ready(CompioTlsStream::connect(
        &pki.connector()?,
        client,
        "localhost",
    )) else {
        return Err(TestError::new("a closed stream completed a handshake"));
    };
    assert_eq!(
        error,
        TlsError::handshake_failed(
            "localhost".to_string(),
            "the stream closed during the handshake".to_string()
        )
    );
    // The accepting side reads first, and its host is empty.
    let ((dialled, _), (accepted, _)) = faulty_ends(3);
    dialled.inner.sim().shutdown();
    let Err(refused) = ready(CompioTlsStream::accept(&pki.acceptor()?, accepted)) else {
        return Err(TestError::new("a closed stream was accepted"));
    };
    assert_eq!(
        refused,
        TlsError::handshake_failed(
            String::new(),
            "the stream closed during the handshake".to_string()
        )
    );
    Ok(())
}

#[test]
fn test_a_failed_write_or_read_fails_the_handshake_with_the_streams_error() -> TestResult {
    let pki = TestPki::new()?;
    // The client's hello cannot be written.
    let ((client, faults), _server) = faulty_ends(4);
    faults.fail_writes.set(true);
    let Err(error) = ready(CompioTlsStream::connect(
        &pki.connector()?,
        client,
        "localhost",
    )) else {
        return Err(TestError::new("a handshake completed over a broken stream"));
    };
    assert_eq!(
        error,
        TlsError::handshake_failed("localhost".to_string(), reason(ErrorKind::BrokenPipe))
    );
    // The hello is written, the answer cannot be read.
    let ((reset, reset_faults), _peer) = faulty_ends(5);
    reset_faults.fail_reads.set(true);
    let Err(reset_error) = ready(CompioTlsStream::connect(
        &pki.connector()?,
        reset,
        "localhost",
    )) else {
        return Err(TestError::new("a handshake completed over a reset stream"));
    };
    assert_eq!(
        reset_error,
        TlsError::handshake_failed("localhost".to_string(), reason(ErrorKind::ConnectionReset))
    );
    Ok(())
}

#[test]
fn test_an_interrupted_read_during_the_handshake_is_retried() -> TestResult {
    let pki = TestPki::new()?;
    let ((client, client_faults), (server, server_faults)) = faulty_ends(6);
    client_faults.interrupt_next_read.set(true);
    server_faults.interrupt_next_read.set(true);
    let (acceptor, connector) = (pki.acceptor()?, pki.connector()?);
    let (server, client) = ready(futures::future::join(
        CompioTlsStream::accept(&acceptor, server),
        CompioTlsStream::connect(&connector, client, "localhost"),
    ));
    let (server, client) = (server?, client?);
    assert!(!client.session().is_handshaking() && !server.session().is_handshaking());
    assert!(
        !client_faults.interrupt_next_read.get() && !server_faults.interrupt_next_read.get(),
        "each side's read was interrupted once"
    );
    Ok(())
}

/// A peer that sends the start of one 16 KiB handshake record, a byte a read, and never its end; every write taken.
#[derive(Debug, Default)]
struct Dribbling {
    reads: usize,
}

impl AsyncRead for Dribbling {
    async fn read<B: IoBufMut>(&mut self, buf: B) -> BufResult<usize, B> {
        // A handshake record (22) of TLS 1.2's version, 16,384 bytes long, then zeros.
        const HEADER: [u8; 5] = [0x16, 0x03, 0x03, 0x40, 0x00];
        let byte = [HEADER.get(self.reads).copied().unwrap_or(0)];
        self.reads += 1;
        let mut source: &[u8] = &byte;
        source.read(buf).await
    }
}

impl AsyncWrite for Dribbling {
    async fn write<T: IoBuf>(&mut self, buf: T) -> BufResult<usize, T> {
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

#[test]
fn test_a_handshake_that_never_completes_is_refused_past_its_turns() -> TestResult {
    let pki = TestPki::new()?;
    let mut dribbling = Dribbling::default();
    let Err(error) = ready(CompioTlsStream::connect(
        &pki.connector()?,
        &mut dribbling,
        "localhost",
    )) else {
        return Err(TestError::new(
            "a handshake completed on a record never whole",
        ));
    };
    assert_eq!(
        error,
        TlsError::handshake_failed(
            "localhost".to_string(),
            "no handshake after 4096 turns".to_string()
        )
    );
    // HANDSHAKE_TURNS_MAX, 4,096: one read a turn.
    assert_eq!(dribbling.reads, 4_096);
    Ok(())
}

#[test]
fn test_a_read_with_no_room_takes_nothing() -> TestResult {
    let pki = TestPki::new()?;
    let (mut server, mut client, _, _) = faulty_pair(&pki)?;
    ready(send(&mut server, b"hi"))?;
    let BufResult(read, _) = ready(client.read(Vec::with_capacity(0)));
    assert_eq!(read.context("a read with no room")?, 0);
    assert_eq!(ready(receive(&mut client, 2))?, b"hi");
    Ok(())
}

#[test]
fn test_a_read_dropped_while_it_waits_leaves_the_stream_readable() -> TestResult {
    let pki = TestPki::new()?;
    let (mut server, mut client, _, _) = faulty_pair(&pki)?;
    // Nothing is in flight, so the read waits on the stream holding the stream's buffer, and is dropped there.
    let waiting = futures::FutureExt::now_or_never(client.read(Vec::with_capacity(8)));
    assert!(waiting.is_none(), "a read with nothing in flight waits");
    ready(send(&mut server, b"hi"))?;
    assert_eq!(ready(receive(&mut client, 2))?, b"hi");
    Ok(())
}

#[test]
fn test_a_stream_closed_without_close_notify_reads_as_unexpected_eof() -> TestResult {
    let pki = TestPki::new()?;
    let (server, mut client, _, _) = faulty_pair(&pki)?;
    // The server's stream ends beneath its session, which sends no close_notify.
    server.get_ref().inner.sim().shutdown();
    let BufResult(read, _) = ready(client.read(Vec::with_capacity(8)));
    let Err(error) = read else {
        return Err(TestError::new("a read past the stream's end gave bytes"));
    };
    assert_eq!(error.kind(), ErrorKind::UnexpectedEof);
    Ok(())
}

#[test]
fn test_a_stream_failing_under_an_open_session_fails_its_reads_and_writes() -> TestResult {
    let pki = TestPki::new()?;
    let (_server, mut client, faults, _) = faulty_pair(&pki)?;
    // A read flushes what the session owes before it waits: a flush that fails fails the read.
    faults.fail_writes.set(true);
    let BufResult(read, _) = ready(client.read(Vec::with_capacity(8)));
    assert_eq!(read.err().map(|e| e.kind()), Some(ErrorKind::BrokenPipe));
    // A write's records that cannot be written fail the write.
    let BufResult(written, _) = ready(client.write(b"x".to_vec()));
    assert_eq!(written.err().map(|e| e.kind()), Some(ErrorKind::BrokenPipe));
    // A read that fails fails the read.
    faults.fail_writes.set(false);
    faults.fail_reads.set(true);
    let BufResult(reset, _) = ready(client.read(Vec::with_capacity(8)));
    assert_eq!(
        reset.err().map(|e| e.kind()),
        Some(ErrorKind::ConnectionReset)
    );
    Ok(())
}

#[test]
fn test_a_write_after_the_streams_shutdown_is_refused() -> TestResult {
    let pki = TestPki::new()?;
    let (_server, mut client, _, _) = faulty_pair(&pki)?;
    ready(client.shutdown()).context("a shutdown")?;
    let BufResult(written, _) = ready(client.write(b"late".to_vec()));
    let Err(error) = written else {
        return Err(TestError::new("a write after close_notify was taken"));
    };
    assert_eq!(error.kind(), ErrorKind::Other);
    assert_eq!(
        error.to_string(),
        TlsError::record_refused("localhost".to_string(), "the session has ended".to_string())
            .to_string()
    );
    Ok(())
}
