/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_transport::{
    ClientHandshake, ServerHandshake, TransportError, TransportErrorEnum,
};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, Credit, ErrorCode, ErrorReplyView, FrameView, Header,
    HeaderView, Hello, MAX_FRAME_SIZE, Welcome,
};

const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

fn server() -> ServerHandshake {
    ServerHandshake::new(
        CLUSTER,
        Welcome::new(2, 9, "s")
            .with_max_frame(1 << 20)
            .with_max_streams(16)
            .with_initial_credit(4),
    )
}

fn hello(min: u16, max: u16) -> Vec<u8> {
    ClientHandshake::new(CLUSTER, Hello::new(min, max, 5, MAX_FRAME_SIZE as u32, "c"))
        .hello_frame()
        .unwrap()
}

/// The answer frame's error code.
fn code(frame: &[u8]) -> ErrorCode {
    let view = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE).unwrap();
    ErrorReplyView::from_frame(&view).unwrap().code()
}

#[test]
fn test_a_hello_is_welcomed_at_the_highest_protocol_both_speak() {
    let answer = server().receive(&hello(1, 3));
    let welcome = answer.outcome().clone().unwrap();
    assert_eq!(welcome.protocol(), 2);
    let frame = answer.frame().unwrap();
    assert_eq!(
        ClientHandshake::new(CLUSTER, Hello::new(1, 3, 5, MAX_FRAME_SIZE as u32, "c"))
            .receive(frame)
            .unwrap(),
        welcome,
        "the client reads the Welcome the server sent"
    );
    assert_eq!(
        server()
            .receive(&hello(1, 1))
            .outcome()
            .clone()
            .unwrap()
            .protocol(),
        1
    );
}

#[test]
fn test_a_hello_above_the_server_is_answered_with_its_error_and_refused() {
    let (frame, outcome) = server().receive(&hello(3, 4)).into_parts();
    assert_eq!(code(&frame.unwrap()), ErrorCode::ProtocolVersion);
    assert!(matches!(
        outcome.unwrap_err().0,
        TransportErrorEnum::Refused {
            code: ErrorCode::ProtocolVersion,
            ..
        }
    ));
}

#[test]
fn test_another_command_or_a_damaged_frame_is_answered_with_a_protocol_error() {
    let credit = Credit::new(1)
        .frame(
            Header::new(CLUSTER, Command::Credit)
                .with_stream(1)
                .with_sequence(1),
        )
        .unwrap();
    let (frame, outcome) = server().receive(&credit).into_parts();
    assert_eq!(code(&frame.unwrap()), ErrorCode::Protocol);
    assert_eq!(
        outcome.unwrap_err(),
        TransportError::UnexpectedCommand(1, 14)
    );
    let mut damaged = hello(1, 1);
    let last = damaged.len() - 1;
    damaged[last] ^= 1;
    let (frame, outcome) = server().receive(&damaged).into_parts();
    assert_eq!(code(&frame.unwrap()), ErrorCode::Protocol);
    // The body's last byte is in its one chunk (PROTOCOL.md section 2.5, step 11).
    assert!(matches!(
        outcome.unwrap_err().0,
        TransportErrorEnum::Protocol {
            error: ProtocolError(ProtocolErrorEnum::Chunk { index: 0, .. })
        }
    ));
    // Another cluster's Hello (step 5): the server's cluster expected, the client's found.
    let foreign =
        ClientHandshake::new(CLUSTER + 1, Hello::new(1, 1, 5, MAX_FRAME_SIZE as u32, "c"))
            .hello_frame()
            .unwrap();
    let (frame, outcome) = server().receive(&foreign).into_parts();
    assert_eq!(code(&frame.unwrap()), ErrorCode::Protocol);
    assert_eq!(
        outcome.unwrap_err(),
        TransportError::Protocol(ProtocolError::WrongCluster(CLUSTER, CLUSTER + 1))
    );
}

#[test]
fn test_a_hello_breaking_its_bodys_rules_is_answered_with_a_protocol_error() {
    // PROTOCOL.md section 5.1: `protocol_min` at least 1, `protocol_max` at least `protocol_min`, `max_frame` from
    // 524,288 to 1,048,576.
    for (hello, refusal) in [
        (
            Hello::new(0, 1, 5, MAX_FRAME_SIZE as u32, "c"),
            ProtocolError::FieldValue(1, "protocol_min", 0),
        ),
        (
            Hello::new(2, 1, 5, MAX_FRAME_SIZE as u32, "c"),
            ProtocolError::FieldValue(1, "protocol_max", 1),
        ),
        (
            Hello::new(1, 1, 5, 524_287, "c"),
            ProtocolError::FieldValue(1, "max_frame", 524_287),
        ),
    ] {
        let frame = ClientHandshake::new(CLUSTER, hello).hello_frame().unwrap();
        let (answer, outcome) = server().receive(&frame).into_parts();
        assert_eq!(code(&answer.unwrap()), ErrorCode::Protocol);
        assert_eq!(outcome.unwrap_err(), TransportError::Protocol(refusal));
    }
    // At the floor exactly, a Hello is welcomed.
    let floor = ClientHandshake::new(CLUSTER, Hello::new(1, 1, 5, 524_288, "c"))
        .hello_frame()
        .unwrap();
    assert_eq!(
        server()
            .receive(&floor)
            .outcome()
            .clone()
            .unwrap()
            .protocol(),
        1
    );
}

#[test]
fn test_the_welcome_is_the_servers_first_frame() {
    let answer = server().receive(&hello(1, 1));
    let header = HeaderView::new(answer.frame().unwrap()).unwrap();
    assert_eq!((header.sequence(), header.stream()), (1, 0));
}

#[test]
fn test_a_first_frame_not_numbered_one_or_naming_a_view_or_replica_is_refused() {
    // PROTOCOL.md section 2.5: a client's frame names no view or replica (step 6), and its first is sequence 1
    // (step 13).
    let hello_with = |header: Header| {
        Hello::new(1, 1, 5, MAX_FRAME_SIZE as u32, "c")
            .frame(header)
            .unwrap()
    };
    let first = Header::new(CLUSTER, Command::Hello).with_sequence(1);
    for (frame, refusal) in [
        (
            hello_with(Header::new(CLUSTER, Command::Hello).with_sequence(2)),
            ProtocolError::SequenceOrder(1, 2),
        ),
        (
            hello_with(first.clone().with_view(3)),
            ProtocolError::FieldValue(1, "view", 3),
        ),
        (
            hello_with(first.with_replica(1)),
            ProtocolError::FieldValue(1, "replica", 1),
        ),
    ] {
        let (answer, outcome) = server().receive(&frame).into_parts();
        assert_eq!(code(&answer.unwrap()), ErrorCode::Protocol);
        assert_eq!(outcome.unwrap_err(), TransportError::Protocol(refusal));
    }
}
