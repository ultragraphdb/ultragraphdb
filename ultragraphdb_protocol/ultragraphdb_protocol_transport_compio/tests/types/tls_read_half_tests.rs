/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A split TLS stream's receiving half reads the peer's plaintext, as the whole stream does.

use std::io::ErrorKind;

use compio::BufResult;
use compio::io::{AsyncRead, AsyncWrite};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transport_tls::TestPki;

use crate::utils::bytes::{receive, send};
use crate::utils::probe::split_pair;
use crate::utils::ready::ready;

#[test]
fn test_the_read_half_reads_the_peers_plaintext() -> TestResult {
    let pki = TestPki::new()?;
    let (mut server, mut read, _write, _probed) = split_pair(&pki)?;
    ready(send(&mut server, b"pong"))?;
    assert_eq!(ready(receive(&mut read, 4))?, b"pong");
    Ok(())
}

#[test]
fn test_a_read_of_the_read_half_with_no_room_takes_nothing() -> TestResult {
    let pki = TestPki::new()?;
    let (mut server, mut read, _write, _probed) = split_pair(&pki)?;
    ready(send(&mut server, b"hi"))?;
    let BufResult(taken, _) = ready(read.read(Vec::with_capacity(0)));
    assert_eq!(taken.context("a read with no room")?, 0);
    assert_eq!(ready(receive(&mut read, 2))?, b"hi");
    Ok(())
}

#[test]
fn test_a_stream_closed_without_close_notify_reads_as_unexpected_eof_on_the_read_half() -> TestResult
{
    let pki = TestPki::new()?;
    let (server, mut read, _write, _probed) = split_pair(&pki)?;
    // The server's stream ends beneath its session, which sends no close_notify.
    server.get_ref().sim().shutdown();
    let BufResult(taken, _) = ready(read.read(Vec::with_capacity(8)));
    let Err(error) = taken else {
        return Err(TestError::new("a read past the stream's end gave bytes"));
    };
    assert_eq!(error.kind(), ErrorKind::UnexpectedEof);
    Ok(())
}

#[test]
fn test_a_failed_read_of_the_stream_fails_the_read_half() -> TestResult {
    let pki = TestPki::new()?;
    let (_server, mut read, _write, probed) = split_pair(&pki)?;
    probed.fail_reads.set(true);
    let BufResult(taken, _) = ready(read.read(Vec::with_capacity(8)));
    assert_eq!(
        taken.err().map(|e| e.kind()),
        Some(ErrorKind::ConnectionReset)
    );
    Ok(())
}

#[test]
fn test_bytes_that_are_no_record_fail_the_read_half() -> TestResult {
    let pki = TestPki::new()?;
    let (_server, mut read, _write, probed) = split_pair(&pki)?;
    probed.garbage.set(true);
    let BufResult(taken, _) = ready(read.read(Vec::with_capacity(8)));
    assert_eq!(taken.err().map(|e| e.kind()), Some(ErrorKind::Other));
    Ok(())
}

#[test]
fn test_the_peers_close_notify_reads_as_the_end_on_the_read_half() -> TestResult {
    let pki = TestPki::new()?;
    let (mut server, mut read, _write, _probed) = split_pair(&pki)?;
    ready(server.shutdown())?;
    let BufResult(taken, _) = ready(read.read(Vec::with_capacity(8)));
    assert_eq!(taken.context("a read after close_notify")?, 0);
    Ok(())
}
