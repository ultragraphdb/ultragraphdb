/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{BodyBuilder, ErrorCode, ErrorReply, RequestTerm};
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
