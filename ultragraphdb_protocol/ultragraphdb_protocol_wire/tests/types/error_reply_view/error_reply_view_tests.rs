/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, ErrorCode, ErrorReply, ErrorReplyView, RequestTermView,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::{
    frame_of, golden, join_sections, put, reframe, split_sections, with_command_header,
};

#[test]
fn test_the_golden_errors_read_their_values() -> TestResult {
    let bytes = golden("error")?.bytes()?;
    let view = ErrorReplyView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(view.code(), ErrorCode::Conflict);
    assert_eq!(view.detail(), 3);
    assert_eq!(view.message(), "conflict on a scanned range");
    assert_eq!(view.value(), None);
    assert_eq!(
        view.to_body()?,
        ErrorReply::new(ErrorCode::Conflict, "conflict on a scanned range").with_detail(3)
    );
    let operation_bytes = golden("error_operation")?.bytes()?;
    let operation_view = ErrorReplyView::from_frame(&frame_of(&operation_bytes)?)?;
    assert_eq!(operation_view.code(), ErrorCode::OperationFailed);
    assert_eq!(
        operation_view.value(),
        Some(RequestTermView::Literal {
            lexical: "done",
            datatype: None
        })
    );
    Ok(())
}

#[test]
fn test_the_value_is_a_term_table_of_at_most_one_entry() -> TestResult {
    // Two entries in the value's table: the value is one term.
    let bytes = golden("error_operation")?.bytes()?;
    // Two IRI records, laid out by PROTOCOL.md section 4.4: kind, direction, six zeros, then the two texts' offsets
    // and lengths.
    let record = |first: u32| {
        let mut r = vec![1, 0, 0, 0, 0, 0, 0, 0];
        for v in [first, 1, first + 1, 0] {
            r.extend_from_slice(&v.to_le_bytes());
        }
        r
    };
    let mut secs = split_sections(&golden("error_operation")?.data()?, 0, 3);
    secs[1] = [record(0), record(1)].concat();
    secs[2] = b"ab".to_vec();
    let two = reframe(&bytes, &join_sections(&[], &secs));
    assert_eq!(
        ErrorReplyView::from_frame(&frame_of(&two)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::CountMismatch {
            command: 16,
            field: "value",
            count: 2,
            expected: 1
        }
    );
    // A table breaking its own rules is refused by them: a record of an unknown kind.
    let mut unknown_kind_secs = split_sections(&golden("error_operation")?.data()?, 0, 3);
    unknown_kind_secs[1][0] = 9;
    let bad = reframe(&bytes, &join_sections(&[], &unknown_kind_secs));
    assert!(matches!(
        ErrorReplyView::from_frame(&frame_of(&bad)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::FieldValue {
            field: "term kind",
            ..
        }
    ));
    // A value rebuilt from its view is the same frame.
    let view = ErrorReplyView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(
        view.to_body()?.frame(golden("error_operation")?.header())?,
        bytes
    );
    Ok(())
}

#[test]
fn test_every_code_reads_and_unused_bytes_are_zero() -> TestResult {
    let bytes = golden("error")?.bytes()?;
    for code in ErrorCode::ALL {
        let changed = with_command_header(&bytes, |h| put(h, 0, code.number().into(), 2));
        assert_eq!(
            ErrorReplyView::from_frame(&frame_of(&changed)?)?.code(),
            code
        );
    }
    for at in [2usize, 7, 16, 127] {
        let changed = with_command_header(&bytes, |h| h[at] = 1);
        assert_eq!(
            ErrorReplyView::from_frame(&frame_of(&changed)?)
                .err()
                .context("a refusal")?
                .0,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    Ok(())
}

#[test]
fn test_an_unknown_code_is_refused() -> TestResult {
    let bytes = golden("error")?.bytes()?;
    for code in [0u64, 24, 0xFFFF] {
        let changed = with_command_header(&bytes, |h| put(h, 0, code, 2));
        assert_eq!(
            ErrorReplyView::from_frame(&frame_of(&changed)?)
                .err()
                .context("a refusal")?
                .0,
            ProtocolErrorEnum::FieldValue {
                command: 16,
                field: "code",
                value: code
            }
        );
    }
    Ok(())
}
