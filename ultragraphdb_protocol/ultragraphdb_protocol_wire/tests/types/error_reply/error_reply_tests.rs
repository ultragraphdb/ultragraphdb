/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{BodyBuilder, ErrorCode, ErrorReply, RequestTerm};

use crate::utils::golden_bodies::golden;

#[test]
fn test_error_replies_build_the_golden_frames() {
    let conflict =
        ErrorReply::new(ErrorCode::Conflict, "conflict on a scanned range").with_detail(3);
    assert_eq!(conflict.code(), ErrorCode::Conflict);
    assert_eq!(conflict.detail(), 3);
    assert_eq!(conflict.message(), "conflict on a scanned range");
    assert_eq!(conflict.value(), None);
    assert!(conflict.retryable());
    assert_eq!(
        conflict.frame(golden("error").header()).unwrap(),
        golden("error").bytes()
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
        failed.frame(golden("error_operation").header()).unwrap(),
        golden("error_operation").bytes()
    );
}

#[test]
fn test_a_protocol_error_is_answered_with_its_text() {
    let reply = ErrorReply::protocol(&ProtocolError::UnknownCommand(20));
    assert_eq!(reply.code(), ErrorCode::Protocol);
    assert_eq!(reply.message(), "unknown command 20");
    assert_eq!(reply.detail(), 0);
    assert_eq!(reply.value(), None);
}
