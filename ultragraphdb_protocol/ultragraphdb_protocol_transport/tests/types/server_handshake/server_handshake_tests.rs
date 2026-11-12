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
use ultragraphdb_tool_bdd::{Context, TestResult};

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

fn hello(min: u16, max: u16) -> TestResult<Vec<u8>> {
    Ok(ClientHandshake::new(
        CLUSTER,
        Hello::new(min, max, 5, u32::try_from(MAX_FRAME_SIZE)?, "c"),
    )
    .hello_frame()?)
}

/// The answer frame's error code.
fn code(frame: &[u8]) -> TestResult<ErrorCode> {
    let view = FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE)?;
    Ok(ErrorReplyView::from_frame(&view)?.code())
}

#[test]
fn test_a_hello_is_welcomed_at_the_highest_protocol_both_speak() -> TestResult {
    let answer = server().receive(&hello(1, 3)?);
    let welcome = answer.outcome().clone()?;
    assert_eq!(welcome.protocol(), 2);
    let frame = answer.frame().context("the Welcome frame")?;
    assert_eq!(
        ClientHandshake::new(
            CLUSTER,
            Hello::new(1, 3, 5, u32::try_from(MAX_FRAME_SIZE)?, "c")
        )
        .receive(frame)?,
        welcome,
        "the client reads the Welcome the server sent"
    );
    assert_eq!(
        server()
            .receive(&hello(1, 1)?)
            .outcome()
            .clone()?
            .protocol(),
        1
    );
    Ok(())
}

#[test]
fn test_a_hello_above_the_server_is_answered_with_its_error_and_refused() -> TestResult {
    let (frame, outcome) = server().receive(&hello(3, 4)?).into_parts();
    let frame = frame.context("the answer frame")?;
    assert_eq!(code(&frame)?, ErrorCode::ProtocolVersion);
    // The refusal returned is the one sent: its code and its message.
    let view = FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)?;
    let sent = ErrorReplyView::from_frame(&view)?;
    assert!(!sent.message().is_empty());
    assert_eq!(
        outcome.err().context("the refusal")?,
        TransportError::refused(ErrorCode::ProtocolVersion, sent.message())
    );
    Ok(())
}

#[test]
fn test_another_command_or_a_damaged_frame_is_answered_with_a_protocol_error() -> TestResult {
    let credit = Credit::new(1).frame(
        Header::new(CLUSTER, Command::Credit)
            .with_stream(1)
            .with_sequence(1),
    )?;
    let (frame, outcome) = server().receive(&credit).into_parts();
    assert_eq!(
        code(&frame.context("the answer frame")?)?,
        ErrorCode::Protocol
    );
    assert_eq!(
        outcome.err().context("the refusal")?,
        TransportError::unexpected_command(1, 14)
    );
    let mut damaged = hello(1, 1)?;
    let last = damaged.len() - 1;
    damaged[last] ^= 1;
    let (damaged_answer, damaged_outcome) = server().receive(&damaged).into_parts();
    assert_eq!(
        code(&damaged_answer.context("the answer frame")?)?,
        ErrorCode::Protocol
    );
    // The body's last byte is in its one chunk (PROTOCOL.md section 2.5, step 11).
    assert!(matches!(
        damaged_outcome.err().context("the refusal")?.0,
        TransportErrorEnum::Protocol {
            error: ProtocolError(ProtocolErrorEnum::Chunk { index: 0, .. })
        }
    ));
    // Another cluster's Hello (step 5): the server's cluster expected, the client's found.
    let foreign = ClientHandshake::new(
        CLUSTER + 1,
        Hello::new(1, 1, 5, u32::try_from(MAX_FRAME_SIZE)?, "c"),
    )
    .hello_frame()?;
    let (foreign_answer, foreign_outcome) = server().receive(&foreign).into_parts();
    assert_eq!(
        code(&foreign_answer.context("the answer frame")?)?,
        ErrorCode::Protocol
    );
    assert_eq!(
        foreign_outcome.err().context("the refusal")?,
        TransportError::protocol(ProtocolError::wrong_cluster(CLUSTER, CLUSTER + 1))
    );
    Ok(())
}

#[test]
fn test_a_hello_breaking_its_bodys_rules_is_answered_with_a_protocol_error() -> TestResult {
    // PROTOCOL.md section 5.1: `protocol_min` at least 1, `protocol_max` at least `protocol_min`, `max_frame` from
    // 524,288 to 1,048,576.
    for (hello, refusal) in [
        (
            Hello::new(0, 1, 5, u32::try_from(MAX_FRAME_SIZE)?, "c"),
            ProtocolError::field_value(1, "protocol_min", 0),
        ),
        (
            Hello::new(2, 1, 5, u32::try_from(MAX_FRAME_SIZE)?, "c"),
            ProtocolError::field_value(1, "protocol_max", 1),
        ),
        (
            Hello::new(1, 1, 5, 524_287, "c"),
            ProtocolError::field_value(1, "max_frame", 524_287),
        ),
    ] {
        let frame = ClientHandshake::new(CLUSTER, hello).hello_frame()?;
        let (answer, outcome) = server().receive(&frame).into_parts();
        assert_eq!(
            code(&answer.context("the answer frame")?)?,
            ErrorCode::Protocol
        );
        assert_eq!(
            outcome.err().context("the refusal")?,
            TransportError::protocol(refusal)
        );
    }
    // At the floor exactly, a Hello is welcomed.
    let floor = ClientHandshake::new(CLUSTER, Hello::new(1, 1, 5, 524_288, "c")).hello_frame()?;
    assert_eq!(server().receive(&floor).outcome().clone()?.protocol(), 1);
    Ok(())
}

#[test]
fn test_the_welcome_is_the_servers_first_frame() -> TestResult {
    let answer = server().receive(&hello(1, 1)?);
    let header = HeaderView::new(answer.frame().context("the Welcome frame")?)?;
    assert_eq!((header.sequence(), header.stream()), (1, 0));
    Ok(())
}

#[test]
fn test_a_first_frame_not_numbered_one_or_naming_a_view_or_replica_is_refused() -> TestResult {
    // PROTOCOL.md section 2.5: a client's frame names no view or replica (step 6), and its first is sequence 1
    // (step 13).
    let hello_with = |header: Header| -> TestResult<Vec<u8>> {
        Ok(Hello::new(1, 1, 5, u32::try_from(MAX_FRAME_SIZE)?, "c").frame(header)?)
    };
    let first = Header::new(CLUSTER, Command::Hello).with_sequence(1);
    for (frame, refusal) in [
        (
            hello_with(Header::new(CLUSTER, Command::Hello).with_sequence(2))?,
            ProtocolError::sequence_order(1, 2),
        ),
        (
            hello_with(first.clone().with_view(3))?,
            ProtocolError::field_value(1, "view", 3),
        ),
        (
            hello_with(first.with_replica(1))?,
            ProtocolError::field_value(1, "replica", 1),
        ),
    ] {
        let (answer, outcome) = server().receive(&frame).into_parts();
        assert_eq!(
            code(&answer.context("the answer frame")?)?,
            ErrorCode::Protocol
        );
        assert_eq!(
            outcome.err().context("the refusal")?,
            TransportError::protocol(refusal)
        );
    }
    Ok(())
}

#[test]
fn test_a_welcome_too_large_to_frame_is_not_built_and_nothing_is_sent() -> TestResult {
    // A server named at 1 MiB: its Welcome cannot be framed, so the client is answered with nothing.
    let oversized = ServerHandshake::new(CLUSTER, Welcome::new(1, 0, "s".repeat(MAX_FRAME_SIZE)));
    let (frame, outcome) = oversized.receive(&hello(1, 1)?).into_parts();
    assert_eq!(frame, None);
    assert!(
        matches!(
            outcome.err().context("the refusal")?.0,
            TransportErrorEnum::FrameNotBuilt {
                command: 2,
                error: ProtocolError(ProtocolErrorEnum::FrameTooLarge {
                    limit: MAX_FRAME_SIZE,
                    size,
                }),
            } if size > MAX_FRAME_SIZE
        ),
        "the Welcome's frame is past the largest"
    );
    Ok(())
}
