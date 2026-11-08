/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A split TLS stream's sending half writes plaintext as records to the stream beneath, flushes it when asked, and on
//! its shutdown sends the session's `close_notify` and shuts the stream's sending side.

use std::io::ErrorKind;

use compio::BufResult;
use compio::io::AsyncWrite;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transport_tls::{TestPki, TlsError};

use crate::utils::bytes::{read_once, receive};
use crate::utils::probe::split_pair;
use crate::utils::ready::ready;

#[test]
fn test_the_write_half_writes_records_the_peer_reads() -> TestResult {
    let pki = TestPki::new()?;
    let (mut server, _read, mut write, probed) = split_pair(&pki)?;
    let before = probed.written.get();
    let BufResult(written, _) = ready(write.write(b"ping".to_vec()));
    assert_eq!(written.context("a write")?, 4);
    assert!(probed.written.get() > before, "records reached the stream");
    assert_eq!(ready(receive(&mut server, 4))?, b"ping");
    Ok(())
}

#[test]
fn test_a_flush_of_the_write_half_flushes_the_stream() -> TestResult {
    let pki = TestPki::new()?;
    let (_server, _read, mut write, probed) = split_pair(&pki)?;
    let before = probed.flushes.get();
    ready(write.flush())?;
    assert_eq!(probed.flushes.get(), before + 1);
    Ok(())
}

#[test]
fn test_a_shutdown_of_the_write_half_sends_close_notify_and_shuts_the_stream() -> TestResult {
    let pki = TestPki::new()?;
    let (mut server, _read, mut write, probed) = split_pair(&pki)?;
    ready(write.shutdown())?;
    assert_eq!(probed.shutdowns.get(), 1, "the stream's sending side shut");
    assert_eq!(
        ready(read_once(&mut server))?,
        0,
        "the peer closed by its close_notify"
    );
    Ok(())
}

#[test]
fn test_a_write_of_the_write_half_after_its_shutdown_is_refused() -> TestResult {
    let pki = TestPki::new()?;
    let (_server, _read, mut write, _probed) = split_pair(&pki)?;
    ready(write.shutdown())?;
    let BufResult(written, _) = ready(write.write(b"late".to_vec()));
    let Err(error) = written else {
        return Err(TestError::new("a write after close_notify was taken"));
    };
    assert_eq!(
        error.to_string(),
        TlsError::record_refused("localhost".to_string(), "the session has ended".to_string())
            .to_string()
    );
    Ok(())
}

#[test]
fn test_a_failed_write_of_the_stream_fails_the_write_half() -> TestResult {
    let pki = TestPki::new()?;
    let (_server, _read, mut write, probed) = split_pair(&pki)?;
    probed.fail_writes.set(true);
    let BufResult(written, _) = ready(write.write(b"x".to_vec()));
    assert_eq!(written.err().map(|e| e.kind()), Some(ErrorKind::BrokenPipe));
    Ok(())
}
