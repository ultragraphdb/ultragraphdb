/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::{Read, Write};

use ultragraphdb_transport_tls::{TestEcho, TestPki};

#[test]
fn test_a_peer_refuses_bytes_that_are_no_tls_and_answers_with_an_alert() {
    let pki = TestPki::new();
    let mut peer = pki.peer(TestEcho::default());
    assert!(!peer.failed());
    // An HTTP request where a TLS client hello belongs: 'G' is no TLS record type.
    peer.write_all(b"GET / HTTP/1.1\r\n\r\n")
        .expect("the peer takes any bytes");
    assert!(peer.failed());
    let mut alert = [0u8; 64];
    let n = peer.read(&mut alert).expect("the server's answer");
    // RFC 8446 section 5.1: an alert is a record of content type 21.
    assert_eq!(alert.first(), Some(&21), "{:?}", &alert[..n]);
}

#[test]
fn test_a_peer_with_nothing_to_send_reads_as_the_end_of_the_stream() {
    let pki = TestPki::new();
    let mut peer = pki.peer(TestEcho::default());
    let mut buf = [0u8; 16];
    assert_eq!(peer.read(&mut buf).expect("a read"), 0);
    assert!(!peer.failed());
}
