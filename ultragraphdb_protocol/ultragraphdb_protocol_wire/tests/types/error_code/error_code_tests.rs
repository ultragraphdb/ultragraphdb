/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::ErrorCode;

#[test]
fn test_codes_keep_their_numbers() {
    // The table of ErrorCode's docs.
    let numbers: Vec<u16> = ErrorCode::ALL.iter().map(|c| c.number()).collect();
    assert_eq!(numbers, (1..=27).collect::<Vec<u16>>());
    assert_eq!(ErrorCode::Protocol.number(), 1);
    assert_eq!(ErrorCode::Conflict.number(), 2);
    assert_eq!(ErrorCode::OperationFailed.number(), 8);
    assert_eq!(ErrorCode::StaleStatement.number(), 9);
    assert_eq!(ErrorCode::SessionBusy.number(), 14);
    assert_eq!(ErrorCode::Unavailable.number(), 17);
    assert_eq!(ErrorCode::NotCommitted.number(), 18);
    assert_eq!(ErrorCode::AuthenticationFailed.number(), 19);
    assert_eq!(ErrorCode::Unauthenticated.number(), 20);
    assert_eq!(ErrorCode::PermissionDenied.number(), 21);
    assert_eq!(ErrorCode::CredentialExpired.number(), 22);
    // PROTOCOL.md §9.4: CrossPartition is 23, not retryable.
    assert_eq!(ErrorCode::CrossPartition.number(), 23);
    assert!(!ErrorCode::CrossPartition.retryable());
    // PROTOCOL.md §9.4: the read path's codes 24 to 27.
    assert_eq!(ErrorCode::TimedOut.number(), 24);
    assert_eq!(ErrorCode::LevelRefused.number(), 25);
    assert_eq!(ErrorCode::WrongPartition.number(), 26);
    assert_eq!(ErrorCode::AheadOfWatermark.number(), 27);
    for code in ErrorCode::ALL {
        assert_eq!(ErrorCode::from_number(code.number()), Some(code));
    }
    assert_eq!(ErrorCode::from_number(0), None);
    assert_eq!(ErrorCode::from_number(28), None);
    assert_eq!(ErrorCode::from_number(u16::MAX), None);
}

#[test]
fn test_the_retryable_codes_are_the_tables() {
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
            ErrorCode::Busy,
            ErrorCode::SessionBusy,
            ErrorCode::NotCommitted,
            ErrorCode::TimedOut,
            ErrorCode::WrongPartition
        ]
    );
}
