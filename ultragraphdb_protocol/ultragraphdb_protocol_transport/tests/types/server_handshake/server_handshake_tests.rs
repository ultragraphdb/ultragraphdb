/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_transport::{
    ClientHandshake, ServerHandshake, TransportError, TransportErrorEnum,
};
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, Credit, ErrorCode, ErrorReplyView, FrameView, Header, Hello,
    Welcome,
};

const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

fn server() -> ServerHandshake {
    ServerHandshake::new(CLUSTER, Welcome::new(2, 9, 1 << 20, 16, 4, "s"))
}

fn hello(min: u16, max: u16) -> Vec<u8> {
    ClientHandshake::new(CLUSTER, Hello::new(min, max, 5, "c"))
        .hello_frame()
        .unwrap()
}

/// The answer frame's error code.
fn code(frame: &[u8]) -> ErrorCode {
    let view = FrameView::validate(frame, CLUSTER).unwrap();
    ErrorReplyView::from_frame(&view).unwrap().code()
}

#[test]
fn test_a_hello_is_welcomed_at_the_highest_protocol_both_speak() {
    let answer = server().receive(&hello(1, 3));
    let welcome = answer.outcome().clone().unwrap();
    assert_eq!(welcome.protocol(), 2);
    let frame = answer.frame().unwrap();
    assert_eq!(
        ClientHandshake::new(CLUSTER, Hello::new(1, 3, 5, "c"))
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
        .frame(Header::new(CLUSTER, Command::Credit).with_stream(1))
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
    assert!(matches!(
        outcome.unwrap_err().0,
        TransportErrorEnum::Protocol {
            error: ProtocolError(_)
        }
    ));
}
