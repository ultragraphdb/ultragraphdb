/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{ClientHandshake, TransportError, TransportErrorEnum};
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, Credit, ErrorCode, ErrorReply, FrameView, Header, HeaderView, Hello,
    Welcome,
};

const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

fn welcome(protocol: u16) -> Vec<u8> {
    Welcome::new(protocol, 9, 1 << 20, 16, 4, "s")
        .frame(Header::new(CLUSTER, Command::Welcome))
        .unwrap()
}

#[test]
fn test_the_hello_frame_names_the_cluster_and_the_release_on_stream_zero() {
    let hello = Hello::new(1, 3, 0x0005_0000, "c");
    let handshake = ClientHandshake::new(CLUSTER, hello.clone());
    assert_eq!(handshake.hello(), &hello);
    let frame = handshake.hello_frame().unwrap();
    let header = HeaderView::new(&frame).unwrap();
    assert_eq!(
        (header.cluster(), header.release(), header.stream()),
        (CLUSTER, 0x0005_0000, 0)
    );
    assert_eq!(
        FrameView::validate(&frame, CLUSTER).unwrap().command(),
        Command::Hello
    );
}

#[test]
fn test_a_welcome_within_the_range_is_taken_and_outside_it_refused() {
    let handshake = ClientHandshake::new(CLUSTER, Hello::new(2, 3, 0, "c"));
    for protocol in [2, 3] {
        assert_eq!(
            handshake.receive(&welcome(protocol)).unwrap().protocol(),
            protocol
        );
    }
    for protocol in [1, 4] {
        assert_eq!(
            handshake.receive(&welcome(protocol)).unwrap_err(),
            TransportError::Protocol(ProtocolError::FieldValue(2, "protocol", protocol.into()))
        );
    }
}

#[test]
fn test_an_error_answer_is_a_refusal_and_another_command_unexpected() {
    let handshake = ClientHandshake::new(CLUSTER, Hello::new(3, 4, 0, "c"));
    let error = ErrorReply::new(ErrorCode::ProtocolVersion, "above 2")
        .with_detail(2)
        .frame(Header::new(CLUSTER, Command::Error))
        .unwrap();
    assert_eq!(
        handshake.receive(&error).unwrap_err(),
        TransportError::Refused(ErrorCode::ProtocolVersion, "above 2")
    );
    let credit = Credit::new(1)
        .frame(Header::new(CLUSTER, Command::Credit))
        .unwrap();
    assert_eq!(
        handshake.receive(&credit).unwrap_err(),
        TransportError::UnexpectedCommand(2, 14)
    );
}

#[test]
fn test_a_damaged_or_foreign_answer_is_a_protocol_error() {
    let handshake = ClientHandshake::new(CLUSTER, Hello::new(1, 1, 0, "c"));
    let mut damaged = welcome(1);
    let last = damaged.len() - 1;
    damaged[last] ^= 1;
    assert!(matches!(
        handshake.receive(&damaged).unwrap_err().0,
        TransportErrorEnum::Protocol { .. }
    ));
    let foreign = ClientHandshake::new(CLUSTER + 1, Hello::new(1, 1, 0, "c"));
    assert!(matches!(
        foreign.receive(&welcome(1)).unwrap_err().0,
        TransportErrorEnum::Protocol { .. }
    ));
}
