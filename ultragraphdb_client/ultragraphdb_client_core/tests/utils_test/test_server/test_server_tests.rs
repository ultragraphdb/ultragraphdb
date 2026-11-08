/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The test server's own rules, apart from the client: what it makes of a frame it cannot read whole or that breaks a
//! header rule, and of a frame with no connection open.

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::ClientHandshake;
use ultragraphdb_protocol_wire::{
    BodyView, Command, ErrorCode, ErrorReplyView, FrameView, HeaderView, Hello, MAX_FRAME_SIZE,
};

use crate::utils::harness::{CLUSTER, UNIX, server, start};
use ultragraphdb_tool_bdd::{Context as _, TestResult};

/// The client's Hello of `cluster`, its first frame.
fn hello(cluster: u64) -> TestResult<Vec<u8>> {
    Ok(
        ClientHandshake::new(cluster, Hello::new(1, 1, 0, MAX_FRAME_SIZE as u32, "c"))
            .hello_frame()?,
    )
}

/// The code and message of the server's Error frame `frame`, and its stream.
fn refusal(frame: &[u8]) -> TestResult<(u64, ErrorCode, String)> {
    let view = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE)?;
    let error = ErrorReplyView::from_frame(&view)?;
    Ok((
        view.header().stream(),
        error.code(),
        error.message().to_string(),
    ))
}

#[test]
fn test_a_frame_with_no_connection_open_is_ignored() -> TestResult {
    let mut server = server();
    server.receive(&hello(CLUSTER)?, start());
    assert!(server.received().is_empty());
    assert_eq!(server.transmit(), None);
    assert_eq!(server.next_deadline(), None);
    assert!(!server.is_closing());
    Ok(())
}

#[test]
fn test_a_frame_shorter_than_its_size_closes_the_connection() -> TestResult {
    let now = start();
    let mut server = server();
    server.connection(now, UNIX);
    let frame = hello(CLUSTER)?;
    // The Hello but its last byte: its header promises one more (PROTOCOL.md section 2.5, step 9).
    let short = frame.len() - 1;
    assert!(short > 256, "the header is whole");
    server.receive(&frame[..short], now);
    assert!(server.is_closing());
    let error = server.transmit().context("the closing Error")?;
    let expected = ProtocolError::length_mismatch(frame.len() as u32, short).to_string();
    assert_eq!(refusal(&error)?, (0, ErrorCode::Protocol, expected));
    assert_eq!(server.transmit(), None, "the Error is the last frame");
    Ok(())
}

#[test]
fn test_a_frame_of_another_cluster_closes_the_connection_at_its_header() -> TestResult {
    let now = start();
    let mut server = server();
    server.connection(now, UNIX);
    server.receive(&hello(CLUSTER + 1)?, now);
    assert!(server.is_closing());
    let error = server.transmit().context("the closing Error")?;
    assert_eq!(HeaderView::new(&error)?.command(), Command::Error.number());
    let expected = ProtocolError::wrong_cluster(CLUSTER, CLUSTER + 1).to_string();
    assert_eq!(refusal(&error)?, (0, ErrorCode::Protocol, expected));
    assert_eq!(server.principal(), None);
    Ok(())
}

/// A client that sends its Hello and never authenticates is given up at the connection's authentication deadline:
/// the server closes with an Error on stream 0.
#[test]
fn test_a_connection_not_authenticated_in_time_is_closed() -> TestResult {
    let now = start();
    let mut server =
        ultragraphdb_client_core::TestServer::new(CLUSTER, crate::utils::harness::scram_welcome());
    server.connection(now, UNIX);
    server.receive(&hello(CLUSTER)?, now);
    let welcome = server.transmit().context("the Welcome")?;
    assert_eq!(
        HeaderView::new(&welcome)?.command(),
        Command::Welcome.number()
    );
    assert!(!server.is_closing());
    let deadline = server
        .next_deadline()
        .context("an authentication deadline")?;
    server.handle_timeout(deadline);
    assert!(server.is_closing());
    let error = server.transmit().context("the closing Error")?;
    let (stream, _, _) = refusal(&error)?;
    assert_eq!(stream, 0);
    assert_eq!(server.transmit(), None, "the Error is the last frame");
    Ok(())
}

/// The server's wall clock follows the time it is handed: a credential issued 7 s after the connection opened expires
/// its life after that moment.
#[test]
fn test_the_servers_clock_follows_the_time_it_is_handed() -> TestResult {
    use std::time::Duration;
    use ultragraphdb_protocol_transport::NetworkFaults;

    use crate::utils::harness::{Harness, config, login, scram_welcome};

    let life = Duration::from_secs(120);
    let mut h = Harness::with(
        config(),
        ultragraphdb_client_core::TestServer::new(CLUSTER, scram_welcome())
            .with_credential_life(life),
        NetworkFaults::none(),
        1,
        login()?,
    );
    h.connect()?;
    h.now += Duration::from_secs(7);
    h.until(|h| h.core.is_ready())?;
    assert_eq!(
        h.core.expires(),
        Some(UNIX + u64::try_from((Duration::from_secs(7) + life).as_micros())?)
    );
    Ok(())
}
