/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, ErrorCode, ErrorReply, ErrorReplyView};

use crate::utils::golden_bodies::{frame_of, golden, put, with_command_header};

#[test]
fn test_the_golden_errors_read_their_values() {
    let bytes = golden("error").bytes();
    let view = ErrorReplyView::from_frame(&frame_of(&bytes)).unwrap();
    assert_eq!(view.code(), ErrorCode::Conflict);
    assert_eq!(view.detail(), 3);
    assert_eq!(view.message(), "conflict on a scanned range");
    assert_eq!(view.value(), None);
    assert_eq!(
        view.to_body().unwrap(),
        ErrorReply::new(ErrorCode::Conflict, "conflict on a scanned range").with_detail(3)
    );
    let bytes = golden("error_operation").bytes();
    let view = ErrorReplyView::from_frame(&frame_of(&bytes)).unwrap();
    assert_eq!(view.code(), ErrorCode::OperationFailed);
    assert_eq!(view.value(), Some("\"done\""));
}

#[test]
fn test_an_unknown_code_is_refused() {
    let bytes = golden("error").bytes();
    for code in [0u64, 18, 0xFFFF] {
        let changed = with_command_header(&bytes, |h| put(h, 0, code, 2));
        assert_eq!(
            ErrorReplyView::from_frame(&frame_of(&changed))
                .unwrap_err()
                .0,
            ProtocolErrorEnum::FieldValue {
                command: 16,
                field: "code",
                value: code
            }
        );
    }
}
