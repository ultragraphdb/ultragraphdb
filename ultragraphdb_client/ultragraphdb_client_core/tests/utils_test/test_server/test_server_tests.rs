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

/// The client's Hello of `cluster`, its first frame.
fn hello(cluster: u64) -> Vec<u8> {
    ClientHandshake::new(cluster, Hello::new(1, 1, 0, MAX_FRAME_SIZE as u32, "c"))
        .hello_frame()
        .unwrap()
}

/// The code and message of the server's Error frame `frame`, and its stream.
fn refusal(frame: &[u8]) -> (u64, ErrorCode, String) {
    let view = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE).unwrap();
    let error = ErrorReplyView::from_frame(&view).unwrap();
    (
        view.header().stream(),
        error.code(),
        error.message().to_string(),
    )
}

#[test]
fn test_a_frame_with_no_connection_open_is_ignored() {
    let mut server = server();
    server.receive(&hello(CLUSTER), start());
    assert!(server.received().is_empty());
    assert_eq!(server.transmit(), None);
    assert_eq!(server.next_deadline(), None);
    assert!(!server.is_closing());
}

#[test]
fn test_a_frame_shorter_than_its_size_closes_the_connection() {
    let now = start();
    let mut server = server();
    server.connection(now, UNIX);
    let frame = hello(CLUSTER);
    // The Hello but its last byte: its header promises one more (PROTOCOL.md section 2.5, step 9).
    let short = frame.len() - 1;
    assert!(short > 256, "the header is whole");
    server.receive(&frame[..short], now);
    assert!(server.is_closing());
    let error = server.transmit().expect("the closing Error");
    let expected = ProtocolError::LengthMismatch(frame.len() as u32, short).to_string();
    assert_eq!(refusal(&error), (0, ErrorCode::Protocol, expected));
    assert_eq!(server.transmit(), None, "the Error is the last frame");
}

#[test]
fn test_a_frame_of_another_cluster_closes_the_connection_at_its_header() {
    let now = start();
    let mut server = server();
    server.connection(now, UNIX);
    server.receive(&hello(CLUSTER + 1), now);
    assert!(server.is_closing());
    let error = server.transmit().expect("the closing Error");
    assert_eq!(
        HeaderView::new(&error).unwrap().command(),
        Command::Error.number()
    );
    let expected = ProtocolError::WrongCluster(CLUSTER, CLUSTER + 1).to_string();
    assert_eq!(refusal(&error), (0, ErrorCode::Protocol, expected));
    assert_eq!(server.principal(), None);
}
