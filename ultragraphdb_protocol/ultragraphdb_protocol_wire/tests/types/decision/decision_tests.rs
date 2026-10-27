/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{Decision, ErrorCode};

#[test]
fn test_decisions_compare_by_outcome_and_reason() {
    assert_eq!(Decision::Committed, Decision::Committed);
    let conflict = Decision::Aborted {
        code: ErrorCode::Conflict,
        detail: 1,
    };
    assert_ne!(conflict, Decision::Committed);
    assert_ne!(
        conflict,
        Decision::Aborted {
            code: ErrorCode::Conflict,
            detail: 2
        }
    );
}
