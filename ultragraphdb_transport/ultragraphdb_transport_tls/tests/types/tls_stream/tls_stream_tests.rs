/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::{self, ErrorKind, Read, Write};

use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transport_tls::{
    HANDSHAKE_TURNS_MAX, READ_TURNS_MAX, RECORD_SIZE_MAX, TestEcho, TestPeer, TestPki, TlsErrorEnum,
};

/// A stream whose every read is interrupted and every write taken: a peer that never answers, read without a timeout.
#[derive(Debug, Default)]
struct Stalled {
    reads: usize,
}

impl Read for Stalled {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        self.reads += 1;
        Err(ErrorKind::Interrupted.into())
    }
}

impl Write for Stalled {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A stream that is closed: every read is the end, every write taken.
#[derive(Debug)]
struct Closed;

impl Read for Closed {
    fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
        Ok(0)
    }
}

impl Write for Closed {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn test_a_stream_reads_and_writes_plaintext_over_its_records() -> TestResult {
    let pki = TestPki::new()?;
    let mut tls = pki
        .connector()?
        .connect(pki.peer(TestEcho::default())?, "localhost")?;
    tls.write_all(b"one ").context("a write")?;
    tls.write_all(b"two\n").context("a write")?;
    tls.flush().context("a flush")?;
    let mut echoed = [0u8; 8];
    tls.read_exact(&mut echoed).context("the echo")?;
    assert_eq!(&echoed, b"one two\n");
    assert!(!tls.get_ref().failed());
    let _stream: &mut TestPeer<TestEcho> = tls.get_mut();
    Ok(())
}

#[test]
fn test_a_stream_closed_without_close_notify_reads_as_unexpected_eof() -> TestResult {
    let pki = TestPki::new()?;
    // The echo peer answers nothing to an empty write and sends no close_notify: its stream simply ends.
    let mut tls = pki
        .connector()?
        .connect(pki.peer(TestEcho::default())?, "localhost")?;
    let Err(error) = tls.read(&mut [0u8; 4]) else {
        return Err(TestError::new("a read past the stream's end gave bytes"));
    };
    assert_eq!(error.kind(), ErrorKind::UnexpectedEof);
    Ok(())
}

#[test]
fn test_a_peers_close_notify_reads_as_the_end() -> TestResult {
    let pki = TestPki::new()?;
    let mut server = TestPeer::new(pki.acceptor()?.session()?, TestEcho::default());
    let mut tls = pki.connector()?.connect(&mut server, "localhost")?;
    tls.write_all(b"x").context("a write")?;
    let mut one = [0u8; 1];
    tls.read_exact(&mut one).context("the echo")?;
    drop(tls);
    // A server session's close_notify, delivered to a client that reads on.
    let mut client = TestPeer::new(pki.connector()?.session("localhost")?, TestEcho::default());
    let mut accepted = pki.acceptor()?.accept(&mut client)?;
    accepted.write_all(b"bye").context("a write")?;
    let mut bye = [0u8; 3];
    accepted.read_exact(&mut bye).context("the echo")?;
    assert_eq!(&bye, b"bye");
    Ok(())
}

#[test]
fn test_a_stream_that_closes_during_the_handshake_fails_it() -> TestResult {
    let pki = TestPki::new()?;
    let Err(error) = pki.connector()?.connect(Closed, "localhost") else {
        return Err(TestError::new("a closed stream completed a handshake"));
    };
    assert!(
        matches!(&error.0, TlsErrorEnum::HandshakeFailed { host, .. } if host == "localhost"),
        "{error:?}"
    );
    let Err(refused) = pki.acceptor()?.accept(Closed) else {
        return Err(TestError::new("a closed stream was accepted"));
    };
    assert!(
        matches!(&refused.0, TlsErrorEnum::HandshakeFailed { host, .. } if host.is_empty()),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn test_a_handshake_that_never_progresses_is_refused_past_its_turns() -> TestResult {
    let pki = TestPki::new()?;
    let mut stalled = Stalled::default();
    let Err(error) = pki.connector()?.connect(&mut stalled, "localhost") else {
        return Err(TestError::new("a stalled stream completed a handshake"));
    };
    assert!(
        matches!(&error.0, TlsErrorEnum::HandshakeFailed { reason, .. } if reason.contains("turns")),
        "{error:?}"
    );
    // HANDSHAKE_TURNS_MAX, 4,096: every read is interrupted, so each turn is one read.
    assert_eq!(stalled.reads, 4096);
    Ok(())
}

#[test]
fn test_the_bounds_are_a_records_ciphertext_and_the_turns_named() {
    // RFC 8446 §5.2: 2^14 bytes of plaintext, 256 of expansion, a 5-byte header.
    assert_eq!(RECORD_SIZE_MAX, 16_645);
    assert_eq!(HANDSHAKE_TURNS_MAX, 4_096);
    assert_eq!(READ_TURNS_MAX, 65_536);
}
