/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::ErrorCode;

#[test]
fn test_codes_keep_their_numbers() {
    // The table of ErrorCode's docs.
    let numbers: Vec<u16> = ErrorCode::ALL.iter().map(|c| c.number()).collect();
    assert_eq!(numbers, (1..=17).collect::<Vec<u16>>());
    assert_eq!(ErrorCode::Protocol.number(), 1);
    assert_eq!(ErrorCode::Conflict.number(), 2);
    assert_eq!(ErrorCode::OperationFailed.number(), 8);
    assert_eq!(ErrorCode::StaleStatement.number(), 9);
    assert_eq!(ErrorCode::Unavailable.number(), 17);
    for code in ErrorCode::ALL {
        assert_eq!(ErrorCode::from_number(code.number()), Some(code));
    }
    assert_eq!(ErrorCode::from_number(0), None);
    assert_eq!(ErrorCode::from_number(18), None);
    assert_eq!(ErrorCode::from_number(u16::MAX), None);
}

#[test]
fn test_conflict_too_old_past_limit_and_busy_are_retryable() {
    let retryable: Vec<ErrorCode> = ErrorCode::ALL
        .into_iter()
        .filter(|c| c.retryable())
        .collect();
    assert_eq!(
        retryable,
        vec![
            ErrorCode::Conflict,
            ErrorCode::TooOld,
            ErrorCode::PastLimit,
            ErrorCode::Busy
        ]
    );
}
