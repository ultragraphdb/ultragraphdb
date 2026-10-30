/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_transport::{ClientHandshake, TransportError, TransportErrorEnum};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, Credit, ErrorCode, ErrorReply, FrameView, Header, HeaderView,
    Hello, HelloView, MAX_FRAME_SIZE, Welcome,
};

const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

/// A Welcome at `protocol`, the server's first frame: sequence 1.
fn welcome(protocol: u16) -> Vec<u8> {
    welcome_numbered(protocol, 1)
}

fn welcome_numbered(protocol: u16, sequence: u128) -> Vec<u8> {
    Welcome::new(protocol, 9, "s")
        .with_max_frame(1 << 20)
        .with_max_streams(16)
        .with_initial_credit(4)
        .frame(Header::new(CLUSTER, Command::Welcome).with_sequence(sequence))
        .unwrap()
}

#[test]
fn test_the_hello_frame_names_the_cluster_and_the_release_on_stream_zero() {
    let hello = Hello::new(1, 3, 0x0005_0000, MAX_FRAME_SIZE as u32, "c");
    let handshake = ClientHandshake::new(CLUSTER, hello.clone());
    assert_eq!(handshake.hello(), &hello);
    let frame = handshake.hello_frame().unwrap();
    let header = HeaderView::new(&frame).unwrap();
    assert_eq!(
        (header.cluster(), header.release(), header.stream()),
        (CLUSTER, 0x0005_0000, 0)
    );
    // PROTOCOL.md section 11.1, step 1: sequence 1, protocol 1, every other header field 0.
    assert_eq!(
        (
            header.sequence(),
            header.protocol(),
            header.view(),
            header.replica(),
            header.flags()
        ),
        (1, 1, 0, 0, 0)
    );
    let view = FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE).unwrap();
    assert_eq!(view.command(), Command::Hello);
    // The command header and section 0 carry the Hello's own values.
    let body = HelloView::from_frame(&view).unwrap();
    assert_eq!(
        (
            body.protocol_min(),
            body.protocol_max(),
            body.release(),
            body.max_frame(),
            body.client()
        ),
        (1, 3, 0x0005_0000, 1_048_576, "c")
    );
}

#[test]
fn test_a_welcome_within_the_range_is_taken_and_outside_it_refused() {
    let handshake = ClientHandshake::new(CLUSTER, Hello::new(2, 3, 0, MAX_FRAME_SIZE as u32, "c"));
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
    let handshake = ClientHandshake::new(CLUSTER, Hello::new(3, 4, 0, MAX_FRAME_SIZE as u32, "c"));
    let error = ErrorReply::new(ErrorCode::ProtocolVersion, "above 2")
        .with_detail(2)
        .frame(Header::new(CLUSTER, Command::Error).with_sequence(1))
        .unwrap();
    assert_eq!(
        handshake.receive(&error).unwrap_err(),
        TransportError::Refused(ErrorCode::ProtocolVersion, "above 2")
    );
    let credit = Credit::new(1)
        .frame(Header::new(CLUSTER, Command::Credit).with_sequence(1))
        .unwrap();
    assert_eq!(
        handshake.receive(&credit).unwrap_err(),
        TransportError::UnexpectedCommand(2, 14)
    );
    // An Error whose code is no code of PROTOCOL.md section 9.4: 0, written over the command header's `code` and the
    // checksum renewed by stamping the sequence again.
    let mut unknown = error.clone();
    unknown[128..130].copy_from_slice(&0u16.to_le_bytes());
    Header::stamp_sequence(&mut unknown, 1).unwrap();
    assert_eq!(
        handshake.receive(&unknown).unwrap_err(),
        TransportError::Protocol(ProtocolError::FieldValue(16, "code", 0))
    );
}

#[test]
fn test_a_damaged_or_foreign_answer_is_a_protocol_error() {
    let handshake = ClientHandshake::new(CLUSTER, Hello::new(1, 1, 0, MAX_FRAME_SIZE as u32, "c"));
    let mut damaged = welcome(1);
    let last = damaged.len() - 1;
    damaged[last] ^= 1;
    // The body's last byte is in its one chunk (PROTOCOL.md section 2.5, step 11).
    assert!(matches!(
        handshake.receive(&damaged).unwrap_err().0,
        TransportErrorEnum::Protocol {
            error: ProtocolError(ProtocolErrorEnum::Chunk { index: 0, .. })
        }
    ));
    let foreign =
        ClientHandshake::new(CLUSTER + 1, Hello::new(1, 1, 0, MAX_FRAME_SIZE as u32, "c"));
    assert_eq!(
        foreign.receive(&welcome(1)).unwrap_err(),
        TransportError::Protocol(ProtocolError::WrongCluster(CLUSTER + 1, CLUSTER))
    );
    // A Welcome breaking its own rule: `max_streams` 0, where at least 1 is due (PROTOCOL.md section 5.2).
    let no_streams = Welcome::new(1, 9, "s")
        .with_max_frame(1 << 20)
        .with_max_streams(0)
        .with_initial_credit(4)
        .frame(Header::new(CLUSTER, Command::Welcome).with_sequence(1))
        .unwrap();
    assert_eq!(
        handshake.receive(&no_streams).unwrap_err(),
        TransportError::Protocol(ProtocolError::FieldValue(2, "max_streams", 0))
    );
}

#[test]
fn test_an_answer_not_numbered_one_is_out_of_sequence() {
    // PROTOCOL.md section 2.5, step 13: the server's first frame is its sequence 1.
    let handshake = ClientHandshake::new(CLUSTER, Hello::new(1, 1, 0, MAX_FRAME_SIZE as u32, "c"));
    for sequence in [0, 2] {
        assert_eq!(
            handshake
                .receive(&welcome_numbered(1, sequence))
                .unwrap_err(),
            TransportError::Protocol(ProtocolError::SequenceOrder(1, sequence))
        );
    }
    assert_eq!(handshake.receive(&welcome(1)).unwrap().protocol(), 1);
}
