/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::net::{TcpListener, TcpStream};

use rustls::ProtocolVersion;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transport_tls::{
    Plaintext, TestLink, TestPipe, TestPki, TlsError, TlsErrorEnum, TlsSession,
};

/// A client session trusting `pki` for `localhost`, and its acceptor's server session.
fn sessions(pki: &TestPki) -> TestResult<(TlsSession, TlsSession)> {
    Ok((
        pki.connector()?.session("localhost")?,
        pki.acceptor()?.session()?,
    ))
}

/// Moves every record each side has to the other, `turns` times. Nothing to move is not passed on, since an empty
/// `receive` tells a session its carrier closed.
fn shuttle(client: &mut TlsSession, server: &mut TlsSession, turns: usize) -> TestResult {
    let mut buf = vec![0u8; 1 << 15];
    for _ in 0..turns {
        let sent = client.transmit(&mut buf);
        if sent > 0 {
            server.receive(buf.get(..sent).context("within the buffer")?)?;
        }
        let answered = server.transmit(&mut buf);
        if answered > 0 {
            client.receive(buf.get(..answered).context("within the buffer")?)?;
        }
    }
    Ok(())
}

/// Every plaintext byte `session` holds.
fn drain(session: &mut TlsSession) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = [0u8; 7];
    while let Plaintext::Read(n) = session.read(&mut buf) {
        out.extend_from_slice(buf.get(..n).unwrap_or_default());
    }
    out
}

#[test]
fn test_a_client_and_a_server_session_complete_tls_1_3_through_bytes_alone() -> TestResult {
    let pki = TestPki::new()?;
    let (mut client, mut server) = sessions(&pki)?;
    assert!(client.is_handshaking() && server.is_handshaking());
    assert!(client.wants_write(), "the client hello waits");
    assert!(!server.wants_write(), "a server speaks second");
    assert_eq!(
        (client.protocol_version(), server.protocol_version()),
        (None, None)
    );
    shuttle(&mut client, &mut server, 3)?;
    assert!(!client.is_handshaking() && !server.is_handshaking());
    assert_eq!(client.protocol_version(), Some(ProtocolVersion::TLSv1_3));
    assert_eq!(server.protocol_version(), Some(ProtocolVersion::TLSv1_3));
    assert_eq!((client.host(), server.host()), ("localhost", ""));
    Ok(())
}

#[test]
fn test_plaintext_crosses_both_ways_and_reads_in_pieces() -> TestResult {
    let pki = TestPki::new()?;
    let (mut client, mut server) = sessions(&pki)?;
    // Sent before the handshake: queued until it completes.
    client.send(b"hello, server\n")?;
    shuttle(&mut client, &mut server, 3)?;
    assert_eq!(drain(&mut server), b"hello, server\n");
    assert_eq!(server.read(&mut [0u8; 4]), Plaintext::Pending);
    server.send(b"hello, client\n")?;
    shuttle(&mut client, &mut server, 1)?;
    assert_eq!(drain(&mut client), b"hello, client\n");
    Ok(())
}

#[test]
fn test_transmit_fills_a_small_buffer_and_keeps_the_rest() -> TestResult {
    let pki = TestPki::new()?;
    let (mut client, _server) = sessions(&pki)?;
    let mut whole = Vec::new();
    let mut piece = [0u8; 10];
    loop {
        let n = client.transmit(&mut piece);
        if n == 0 {
            break;
        }
        assert!(n <= piece.len());
        whole.extend_from_slice(piece.get(..n).context("within the piece")?);
    }
    // RFC 8446 §5.1: a handshake record, type 22, carrying the client hello.
    assert_eq!(whole.first(), Some(&22));
    assert!(whole.len() > piece.len(), "{} bytes", whole.len());
    assert!(!client.wants_write());
    assert!(client.wants_read());
    Ok(())
}

#[test]
fn test_a_close_notify_reads_as_closed_and_a_bare_close_as_truncated() -> TestResult {
    let pki = TestPki::new()?;
    let (mut client, mut server) = sessions(&pki)?;
    shuttle(&mut client, &mut server, 3)?;
    client.close();
    shuttle(&mut client, &mut server, 1)?;
    assert_eq!(server.read(&mut [0u8; 4]), Plaintext::Closed);
    assert!(!server.wants_read(), "nothing comes after close_notify");
    assert!(
        client.wants_read(),
        "the client still reads the server's records"
    );

    let (mut bare_client, mut bare_server) = sessions(&pki)?;
    shuttle(&mut bare_client, &mut bare_server, 3)?;
    // The carrier closed: no close_notify came first.
    bare_server.receive(&[])?;
    assert_eq!(bare_server.read(&mut [0u8; 4]), Plaintext::Truncated);
    Ok(())
}

#[test]
fn test_bytes_that_are_no_tls_fail_the_handshake_naming_the_host() -> TestResult {
    let pki = TestPki::new()?;
    let (mut client, mut server) = sessions(&pki)?;
    let Err(error) = server.receive(b"GET / HTTP/1.1\r\n\r\n") else {
        return Err(TestError::new(
            "an HTTP request was taken as a client hello",
        ));
    };
    assert!(
        matches!(&error.0, TlsErrorEnum::HandshakeFailed { host, .. } if host.is_empty()),
        "{error:?}"
    );
    // The refusal leaves the alert for the peer: a record of type 21.
    let mut alert = [0u8; 64];
    assert!(server.transmit(&mut alert) > 0);
    assert_eq!(alert.first(), Some(&21));
    let _hello: usize = client.transmit(&mut [0u8; 1 << 12]);
    let Err(alerted) = client.receive(alert.get(..7).context("an alert record")?) else {
        return Err(TestError::new("the server's alert was taken"));
    };
    assert!(
        matches!(&alerted.0, TlsErrorEnum::HandshakeFailed { host, .. } if host == "localhost"),
        "{alerted:?}"
    );
    Ok(())
}

#[test]
fn test_a_damaged_record_after_the_handshake_is_refused_as_a_record() -> TestResult {
    let pki = TestPki::new()?;
    let (mut client, mut server) = sessions(&pki)?;
    shuttle(&mut client, &mut server, 3)?;
    client.send(b"secret")?;
    let mut buf = vec![0u8; 1 << 12];
    let n = client.transmit(&mut buf);
    let last = n.checked_sub(1).context("a record")?;
    let byte = buf.get_mut(last).context("its last byte")?;
    *byte ^= 0xFF;
    let Err(error) = server.receive(buf.get(..n).context("the record")?) else {
        return Err(TestError::new("a record failing its tag was decrypted"));
    };
    assert!(
        matches!(&error.0, TlsErrorEnum::RecordRefused { host, .. } if host.is_empty()),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn test_a_tls_1_2_client_is_refused_by_a_tls_1_3_server_session() -> TestResult {
    let pki = TestPki::new()?;
    let mut client = pki.client_tls12("localhost")?;
    let mut server = pki.acceptor()?.session()?;
    let mut buf = vec![0u8; 1 << 15];
    let n = client.transmit(&mut buf);
    let Err(error) = server.receive(buf.get(..n).context("the client hello")?) else {
        return Err(TestError::new("a TLS 1.2 client hello was taken"));
    };
    assert!(
        matches!(&error.0, TlsErrorEnum::HandshakeFailed { .. }),
        "{error:?}"
    );
    let alert = server.transmit(&mut buf);
    let Err(refused) = client.receive(buf.get(..alert).context("the alert")?) else {
        return Err(TestError::new("the client took the refusal"));
    };
    // rustls's text for the protocol_version alert (RFC 8446 §6.2).
    assert!(
        matches!(&refused.0, TlsErrorEnum::HandshakeFailed { reason, .. } if reason.contains("ProtocolVersion")),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn test_the_same_exchange_runs_over_a_pipe_and_over_a_tcp_socket() -> TestResult {
    let pki = TestPki::new()?;
    let (client, server) = sessions(&pki)?;
    let (near, far) = TestPipe::pair();
    let echoed = TestLink::exchange_line(
        &mut TestLink::new(client, near),
        &mut TestLink::new(server, far),
        b"over a pipe\n",
    )?;
    assert_eq!(echoed, b"over a pipe\n");

    let listener = TcpListener::bind("127.0.0.1:0").context("a loopback port")?;
    let near_tcp = TcpStream::connect(listener.local_addr().context("its address")?)
        .context("the backlog takes the connection")?;
    let (far_tcp, _) = listener.accept().context("the connection")?;
    near_tcp.set_nonblocking(true).context("non-blocking")?;
    far_tcp.set_nonblocking(true).context("non-blocking")?;
    let (client_session, server_session) = sessions(&pki)?;
    let mut client_link = TestLink::new(client_session, near_tcp);
    let mut server_link = TestLink::new(server_session, far_tcp);
    let echoed_tcp = TestLink::exchange_line(&mut client_link, &mut server_link, b"over TCP\n")?;
    assert_eq!(echoed_tcp, b"over TCP\n");
    assert_eq!(
        client_link.session().protocol_version(),
        Some(ProtocolVersion::TLSv1_3)
    );
    Ok(())
}

/// The refusal of a send on a session that has ended, the host its own.
fn ended(host: &str) -> TlsError {
    TlsError::record_refused(host.to_string(), "the session has ended".to_string())
}

#[test]
fn test_a_send_after_the_sessions_own_close_notify_is_refused() -> TestResult {
    let pki = TestPki::new()?;
    let (mut client, mut server) = sessions(&pki)?;
    shuttle(&mut client, &mut server, 3)?;
    client.send(b"before")?;
    client.close();
    assert_eq!(client.send(b"after"), Err(ended("localhost")));
    // An empty send is refused as well: the session takes no more plaintext.
    assert_eq!(client.send(b""), Err(ended("localhost")));
    // What was sent before the close still arrives, then the close.
    shuttle(&mut client, &mut server, 1)?;
    assert_eq!(drain(&mut server), b"before");
    Ok(())
}

#[test]
fn test_a_send_after_the_peers_close_notify_is_refused() -> TestResult {
    let pki = TestPki::new()?;
    let (mut client, mut server) = sessions(&pki)?;
    shuttle(&mut client, &mut server, 3)?;
    server.send(b"open")?;
    client.close();
    shuttle(&mut client, &mut server, 1)?;
    assert_eq!(server.send(b"after"), Err(ended("")));
    // And from the other side: the server's close_notify ends the client's session.
    let (mut other_client, mut other_server) = sessions(&pki)?;
    shuttle(&mut other_client, &mut other_server, 3)?;
    other_server.close();
    shuttle(&mut other_client, &mut other_server, 1)?;
    assert_eq!(other_client.send(b"after"), Err(ended("localhost")));
    Ok(())
}

#[test]
fn test_a_send_after_a_fatal_refusal_is_refused() -> TestResult {
    let pki = TestPki::new()?;
    // After the handshake: a damaged record.
    let (mut client, mut server) = sessions(&pki)?;
    shuttle(&mut client, &mut server, 3)?;
    client.send(b"secret")?;
    let mut buf = vec![0u8; 1 << 12];
    let n = client.transmit(&mut buf);
    let last = n.checked_sub(1).context("a record")?;
    let byte = buf.get_mut(last).context("its last byte")?;
    *byte ^= 0xFF;
    let refused = server.receive(buf.get(..n).context("the record")?);
    assert!(
        matches!(&refused, Err(TlsError(TlsErrorEnum::RecordRefused { host, .. })) if host.is_empty()),
        "{refused:?}"
    );
    assert_eq!(server.send(b"after"), Err(ended("")));
    // During the handshake: bytes that are no TLS.
    let (mut fresh_client, _) = sessions(&pki)?;
    let _hello = fresh_client.transmit(&mut buf);
    let garbage = fresh_client.receive(b"GET / HTTP/1.1\r\n\r\n");
    assert!(
        matches!(&garbage, Err(TlsError(TlsErrorEnum::HandshakeFailed { host, .. })) if host == "localhost"),
        "{garbage:?}"
    );
    assert_eq!(fresh_client.send(b"after"), Err(ended("localhost")));
    Ok(())
}

#[test]
fn test_a_send_on_a_live_session_is_taken_before_and_after_the_handshake() -> TestResult {
    let pki = TestPki::new()?;
    let (mut client, mut server) = sessions(&pki)?;
    // Before the handshake completes, plaintext is queued for it.
    client.send(b"early")?;
    shuttle(&mut client, &mut server, 3)?;
    client.send(b" late")?;
    shuttle(&mut client, &mut server, 1)?;
    assert_eq!(drain(&mut server), b"early late");
    Ok(())
}
