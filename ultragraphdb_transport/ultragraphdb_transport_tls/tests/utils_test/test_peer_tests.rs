/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::{Read, Write};

use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transport_tls::{TestEcho, TestPki};

#[test]
fn test_a_peer_refuses_bytes_that_are_no_tls_and_answers_with_an_alert() -> TestResult {
    let pki = TestPki::new()?;
    let mut peer = pki.peer(TestEcho::default())?;
    assert!(!peer.failed());
    // An HTTP request where a TLS client hello belongs: 'G' is no TLS record type.
    peer.write_all(b"GET / HTTP/1.1\r\n\r\n")
        .context("the peer takes any bytes")?;
    assert!(peer.failed());
    let mut alert = [0u8; 64];
    let n = peer.read(&mut alert).context("the server's answer")?;
    // RFC 8446 section 5.1: an alert is a record of content type 21.
    assert_eq!(alert.first(), Some(&21), "{:?}", &alert[..n]);
    // A failed peer takes what follows and answers nothing more.
    peer.write_all(b"more")
        .context("the peer takes any bytes")?;
    assert!(peer.failed());
    let rest = peer.read(&mut alert).context("the rest of the answer")?;
    assert_eq!(rest, 0, "{:?}", &alert[..rest]);
    Ok(())
}

#[test]
fn test_a_peer_hands_its_application_the_plaintext_and_holds_its_session() -> TestResult {
    let pki = TestPki::new()?;
    // A cursor as the application: it holds what it is written and answers nothing past its end.
    let peer = pki.peer(std::io::Cursor::new(Vec::new()))?;
    let mut tls = pki.connector()?.connect(peer, "localhost")?;
    tls.write_all(b"hello").context("a write")?;
    tls.flush().context("a flush")?;
    let served = tls.get_ref();
    assert_eq!(served.application().get_ref(), b"hello");
    assert!(!served.session().is_handshaking());
    assert_eq!(served.session().host(), "");
    assert!(!served.failed());
    Ok(())
}

#[test]
fn test_a_peer_with_nothing_to_send_reads_as_the_end_of_the_stream() -> TestResult {
    let pki = TestPki::new()?;
    let mut peer = pki.peer(TestEcho::default())?;
    let mut buf = [0u8; 16];
    assert_eq!(peer.read(&mut buf).context("a read")?, 0);
    assert!(!peer.failed());
    Ok(())
}
