/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_protocol_wire::{ConflictKind, ErrorCode, WriteSetRecordView};
use ultragraphdb_transactions_commit::{CommitError, CommitErrorEnum};

fn malformed() -> CommitError {
    let cause = WriteSetRecordView::new(&[1, 2, 3]).expect_err("three bytes are no record");
    CommitError::Malformed(cause)
}

#[test]
fn test_each_constructor_builds_its_variant() {
    assert_eq!(
        CommitError::Conflict(ConflictKind::Range).0,
        CommitErrorEnum::Conflict {
            kind: ConflictKind::Range
        }
    );
    assert_eq!(
        CommitError::TooOld(4, 10).0,
        CommitErrorEnum::TooOld {
            read_version: 4,
            floor: 10
        }
    );
    assert_eq!(
        CommitError::TooLarge(2_000_000, 1_048_576).0,
        CommitErrorEnum::TooLarge {
            size: 2_000_000,
            limit: 1_048_576
        }
    );
    assert!(matches!(malformed().0, CommitErrorEnum::Malformed { .. }));
    assert_eq!(CommitError::Unsealed().0, CommitErrorEnum::Unsealed);
    assert_eq!(CommitError::Unavailable().0, CommitErrorEnum::Unavailable);
}

#[test]
fn test_each_reason_has_its_code_and_detail() {
    // The codes and details as ErrorCode's table in ultragraphdb_protocol_wire states them; a conflict's detail is its
    // kind's number there (Range is 3, Drop 6).
    assert_eq!(
        CommitError::Conflict(ConflictKind::Range).code(),
        ErrorCode::Conflict
    );
    assert_eq!(CommitError::Conflict(ConflictKind::Range).detail(), 3);
    assert_eq!(CommitError::Conflict(ConflictKind::Drop).detail(), 6);
    assert_eq!(CommitError::TooOld(4, 10).code(), ErrorCode::TooOld);
    assert_eq!(CommitError::TooOld(4, 10).detail(), 0);
    assert_eq!(
        CommitError::TooLarge(2_000_000, 1_048_576).code(),
        ErrorCode::TooLarge
    );
    assert_eq!(
        CommitError::TooLarge(2_000_000, 1_048_576).detail(),
        1_048_576
    );
    assert_eq!(malformed().code(), ErrorCode::Protocol);
    assert_eq!(malformed().detail(), 0);
    assert_eq!(CommitError::Unsealed().code(), ErrorCode::Protocol);
    assert_eq!(CommitError::Unsealed().detail(), 0);
    assert_eq!(CommitError::Unavailable().code(), ErrorCode::Unavailable);
    assert_eq!(CommitError::Unavailable().detail(), 0);
}

#[test]
fn test_only_a_conflict_and_too_old_are_retryable() {
    // The spec's "Aborts name their reason": conflict and too old are retryable, too large is not; unavailable is the
    // wire's non-retryable Unavailable, and a malformed or unsealed hand-over is a programming error.
    assert!(CommitError::Conflict(ConflictKind::List).is_retryable());
    assert!(CommitError::TooOld(4, 10).is_retryable());
    assert!(!CommitError::TooLarge(2_000_000, 1_048_576).is_retryable());
    assert!(!malformed().is_retryable());
    assert!(!CommitError::Unsealed().is_retryable());
    assert!(!CommitError::Unavailable().is_retryable());
}

#[test]
fn test_each_reason_displays_its_text() {
    // Dgraph's texts for a conflict (QuadError::Aborted's) and for too old (ErrTsTooOld's, as StoreError has it).
    assert_eq!(
        CommitError::Conflict(ConflictKind::List).to_string(),
        "Transaction has been aborted. Please retry"
    );
    assert_eq!(
        CommitError::TooOld(4, 10).to_string(),
        "Transaction is too old"
    );
    assert_eq!(
        CommitError::TooLarge(2_000_000, 1_048_576).to_string(),
        "the write set takes a round of 2000000 bytes, over the round bound of 1048576"
    );
    assert!(
        malformed()
            .to_string()
            .starts_with("a write-set record is malformed: "),
        "{}",
        malformed()
    );
    assert_eq!(
        CommitError::Unsealed().to_string(),
        "a read set reached the commit step unsealed"
    );
    assert_eq!(
        CommitError::Unavailable().to_string(),
        "the node refuses writes after a failed log sync, until it restarts"
    );
}

#[test]
fn test_only_a_malformed_record_has_a_source() {
    assert!(malformed().source().is_some());
    assert!(CommitError::Unsealed().source().is_none());
}

#[test]
fn test_the_operation_refusals_carry_their_codes_and_what_was_found() {
    let failed = CommitError::OperationFailed(Some("\"done\"".to_string()));
    assert_eq!(
        failed.0,
        CommitErrorEnum::OperationFailed {
            found: Some("\"done\"".to_string())
        }
    );
    assert_eq!(failed.code(), ErrorCode::OperationFailed);
    assert_eq!(failed.detail(), 0);
    assert!(!failed.is_retryable());
    assert_eq!(failed.found(), Some("\"done\""));
    assert_eq!(
        failed.to_string(),
        "the compare-and-set did not hold: \"done\" is held"
    );
    let nothing = CommitError::OperationFailed(None);
    assert_eq!(nothing.found(), None);
    assert_eq!(
        nothing.to_string(),
        "the compare-and-set did not hold: no value is held"
    );
    let refused = CommitError::Refused("no".to_string());
    assert_eq!(
        refused.0,
        CommitErrorEnum::Refused {
            reason: "no".to_string()
        }
    );
    assert_eq!(refused.code(), ErrorCode::Refused);
    assert_eq!(refused.detail(), 0);
    assert!(!refused.is_retryable());
    assert_eq!(refused.found(), None);
    assert_eq!(
        refused.to_string(),
        "the commit-time operation was refused: no"
    );
    assert!(refused.source().is_none());
    assert_eq!(CommitError::Unavailable().found(), None);
}
