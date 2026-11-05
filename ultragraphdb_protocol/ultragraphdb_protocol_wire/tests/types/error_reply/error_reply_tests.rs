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
