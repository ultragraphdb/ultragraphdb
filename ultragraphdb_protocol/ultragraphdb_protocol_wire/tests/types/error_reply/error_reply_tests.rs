/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    BodyBuilder, DETAIL_TRANSACTION_NOT_HELD, ErrorCode, ErrorReply, RequestTerm,
};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::golden;

#[test]
fn test_error_replies_build_the_golden_frames() -> TestResult {
    let conflict =
        ErrorReply::new(ErrorCode::Conflict, "conflict on a scanned range").with_detail(3);
    assert_eq!(conflict.code(), ErrorCode::Conflict);
    assert_eq!(conflict.detail(), 3);
    assert_eq!(conflict.message(), "conflict on a scanned range");
    assert_eq!(conflict.value(), None);
    assert!(conflict.retryable());
    assert_eq!(
        conflict.frame(golden("error")?.header())?,
        golden("error")?.bytes()?
    );
    let failed = ErrorReply::new(
        ErrorCode::OperationFailed,
        "compare-and-set found another value",
    )
    .with_value(RequestTerm::Literal {
        lexical: "done".into(),
        datatype: None,
    });
    assert_eq!(
        failed.value(),
        Some(&RequestTerm::Literal {
            lexical: "done".into(),
            datatype: None
        })
    );
    assert!(!failed.retryable());
    assert_eq!(
        failed.frame(golden("error_operation")?.header())?,
        golden("error_operation")?.bytes()?
    );
    Ok(())
}

#[test]
fn test_a_protocol_error_is_answered_with_its_text() {
    let reply = ErrorReply::protocol(&ProtocolError::unknown_command(20));
    assert_eq!(reply.code(), ErrorCode::Protocol);
    assert_eq!(reply.message(), "unknown command 20");
    assert_eq!(reply.detail(), 0);
    assert_eq!(reply.value(), None);
}

#[test]
fn test_a_detail_its_code_refuses_builds_no_frame() -> TestResult {
    use ultragraphdb_error_protocol::ProtocolErrorEnum;
    use ultragraphdb_protocol_wire::{
        BodyView, Command, ErrorReplyView, FrameView, Header, MAX_FRAME_SIZE,
    };
    // PROTOCOL.md section 9.4: WrongPartition's bucket takes bits 0 to 23 and its partition bits 32 to 63.
    let header = || Header::new(1, Command::Error);
    let refused = ErrorReply::new(ErrorCode::WrongPartition, "moved").with_detail(1 << 24);
    assert_eq!(
        refused.frame(header()).err().map(|e| e.0),
        Some(ProtocolErrorEnum::FieldValue {
            command: Command::Error.number(),
            field: "detail",
            value: 1 << 24
        })
    );
    // A bucket and a partition in their bits build, and the view reads the frame back.
    let held =
        ErrorReply::new(ErrorCode::WrongPartition, "moved").with_detail((7 << 32) | 0x00FF_FFFF);
    let bytes = held.frame(header())?;
    let view = ErrorReplyView::from_frame(&FrameView::validate(&bytes, 1, MAX_FRAME_SIZE)?)?;
    assert_eq!(view.detail(), (7 << 32) | 0x00FF_FFFF);
    Ok(())
}

#[test]
fn test_a_transaction_not_held_is_a_protocol_error_of_detail_1() {
    let reply = ErrorReply::transaction_not_held("no such transaction");
    assert_eq!(reply.code(), ErrorCode::Protocol);
    assert_eq!(reply.detail(), DETAIL_TRANSACTION_NOT_HELD);
    assert_eq!(reply.detail(), 1);
    assert_eq!(reply.message(), "no such transaction");
}

#[test]
fn test_a_schemas_and_a_querys_refusal_build_their_golden_frames() -> TestResult {
    let schema = ErrorReply::new(
        ErrorCode::SchemaRefused,
        "E101 1:1 unknown type; E102 2:1 unknown unit",
    )
    .with_detail(2);
    assert_eq!(
        schema.frame(golden("error_schema")?.header())?,
        golden("error_schema")?.bytes()?
    );
    let query = ErrorReply::new(
        ErrorCode::QueryRefused,
        "S012 1:8 a feature outside the core",
    )
    .with_detail((2 << 16) | 12);
    assert_eq!(
        query.frame(golden("error_query")?.header())?,
        golden("error_query")?.bytes()?
    );
    Ok(())
}

/// The message of `frame`, an Error within the floor of `max_frame`.
fn message_within_the_floor(frame: &[u8]) -> TestResult<String> {
    use ultragraphdb_protocol_wire::{BodyView, ErrorReplyView, FrameView, MAX_FRAME_FLOOR};
    let view = FrameView::validate(frame, 1, MAX_FRAME_FLOOR)?;
    Ok(ErrorReplyView::from_frame(&view)?
        .to_body()?
        .message()
        .to_string())
}

/// An Error frame fits the floor of `max_frame`, which every peer takes: a message past what that frame leaves it is cut
/// at its last character boundary within it, and one at the limit goes whole (`PROTOCOL.md` §9.4).
#[test]
fn test_a_message_past_the_floor_is_cut_to_fit_it() -> TestResult {
    use ultragraphdb_protocol_wire::{Command, Header, MAX_FRAME_FLOOR};
    let header = || Header::new(1, Command::Error);
    let refusal = |message: &str| ErrorReply::new(ErrorCode::Protocol, message);
    let past = "a".repeat(MAX_FRAME_FLOOR);
    let cut = message_within_the_floor(&refusal(&past).frame(header())?)?;
    assert!(cut.len() < past.len(), "the message is cut");
    assert!(past.starts_with(&cut), "the cut keeps the message's start");
    let at = "a".repeat(cut.len());
    assert_eq!(
        message_within_the_floor(&refusal(&at).frame(header())?)?,
        at,
        "a message at the limit goes whole"
    );
    let over = "a".repeat(cut.len() + 1);
    assert_eq!(
        message_within_the_floor(&refusal(&over).frame(header())?)?,
        at,
        "one byte past the limit is cut back to it"
    );
    // A character of three bytes: the cut ends on its boundary, within one character of the limit.
    let euros = "€".repeat(MAX_FRAME_FLOOR / 3);
    let cut_euros = message_within_the_floor(&refusal(&euros).frame(header())?)?;
    assert!(euros.starts_with(&cut_euros));
    assert!(cut_euros.len() <= cut.len() && cut.len() - cut_euros.len() < "€".len());
    Ok(())
}

/// A failed compare-and-set's value keeps its place beside a message cut to fit the floor of `max_frame`.
#[test]
fn test_a_cut_message_leaves_the_value_whole() -> TestResult {
    use ultragraphdb_protocol_wire::{
        BodyView, Command, ErrorReplyView, FrameView, Header, MAX_FRAME_FLOOR,
    };
    let value = RequestTerm::Literal {
        lexical: "done".repeat(1_000),
        datatype: None,
    };
    let failed = ErrorReply::new(ErrorCode::OperationFailed, "b".repeat(MAX_FRAME_FLOOR))
        .with_value(value.clone());
    let frame = failed.frame(Header::new(1, Command::Error))?;
    let read =
        ErrorReplyView::from_frame(&FrameView::validate(&frame, 1, MAX_FRAME_FLOOR)?)?.to_body()?;
    assert_eq!(read.value(), Some(&value));
    assert!(failed.message().starts_with(read.message()));
    assert!(read.message().len() < failed.message().len());
    Ok(())
}
