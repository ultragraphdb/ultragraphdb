/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::utils::fixture::Refused;
use std::error::Error;
use ultragraphdb_tool_bdd::TestResult;

use ultragraphdb_protocol_wire::{ConflictKind, ErrorCode, WriteSetRecordView};
use ultragraphdb_transactions_commit::{CommitError, CommitErrorEnum};

fn malformed() -> TestResult<CommitError> {
    let cause = WriteSetRecordView::new(&[1, 2, 3]).refused("three bytes are no record")?;
    Ok(CommitError::malformed(cause))
}

#[test]
fn test_each_constructor_builds_its_variant() -> TestResult {
    assert_eq!(
        CommitError::conflict(ConflictKind::Range).0,
        CommitErrorEnum::Conflict {
            kind: ConflictKind::Range
        }
    );
    assert_eq!(
        CommitError::too_old(4, 10).0,
        CommitErrorEnum::TooOld {
            read_version: 4,
            floor: 10
        }
    );
    assert_eq!(
        CommitError::too_large(2_000_000, 1_048_576).0,
        CommitErrorEnum::TooLarge {
            size: 2_000_000,
            limit: 1_048_576
        }
    );
    assert!(matches!(malformed()?.0, CommitErrorEnum::Malformed { .. }));
    assert_eq!(CommitError::unsealed().0, CommitErrorEnum::Unsealed);
    assert_eq!(CommitError::unavailable().0, CommitErrorEnum::Unavailable);
    Ok(())
}

#[test]
fn test_each_reason_has_its_code_and_detail() -> TestResult {
    // The codes and details as ErrorCode's table in ultragraphdb_protocol_wire states them; a conflict's detail is its
    // kind's number there (Range is 3, Drop 6).
    assert_eq!(
        CommitError::conflict(ConflictKind::Range).code(),
        ErrorCode::Conflict
    );
    assert_eq!(CommitError::conflict(ConflictKind::Range).detail(), 3);
    assert_eq!(CommitError::conflict(ConflictKind::Drop).detail(), 6);
    assert_eq!(CommitError::too_old(4, 10).code(), ErrorCode::TooOld);
    assert_eq!(CommitError::too_old(4, 10).detail(), 0);
    assert_eq!(
        CommitError::too_large(2_000_000, 1_048_576).code(),
        ErrorCode::TooLarge
    );
    assert_eq!(
        CommitError::too_large(2_000_000, 1_048_576).detail(),
        1_048_576
    );
    assert_eq!(malformed()?.code(), ErrorCode::Protocol);
    assert_eq!(malformed()?.detail(), 0);
    assert_eq!(CommitError::unsealed().code(), ErrorCode::Protocol);
    assert_eq!(CommitError::unsealed().detail(), 0);
    assert_eq!(CommitError::unavailable().code(), ErrorCode::Unavailable);
    assert_eq!(CommitError::unavailable().detail(), 0);
    Ok(())
}

#[test]
fn test_only_a_conflict_and_too_old_are_retryable() -> TestResult {
    // The spec's "Aborts name their reason": conflict and too old are retryable, too large is not; unavailable is the
    // wire's non-retryable Unavailable, and a malformed or unsealed hand-over is a programming error.
    assert!(CommitError::conflict(ConflictKind::List).is_retryable());
    assert!(CommitError::too_old(4, 10).is_retryable());
    assert!(!CommitError::too_large(2_000_000, 1_048_576).is_retryable());
    assert!(!malformed()?.is_retryable());
    assert!(!CommitError::unsealed().is_retryable());
    assert!(!CommitError::unavailable().is_retryable());
    Ok(())
}

#[test]
fn test_each_reason_displays_its_text() -> TestResult {
    // Dgraph's texts for a conflict (QuadError::aborted's) and for too old (ErrTsTooOld's, as StoreError has it).
    assert_eq!(
        CommitError::conflict(ConflictKind::List).to_string(),
        "Transaction has been aborted. Please retry"
    );
    assert_eq!(
        CommitError::too_old(4, 10).to_string(),
        "Transaction is too old"
    );
    assert_eq!(
        CommitError::too_large(2_000_000, 1_048_576).to_string(),
        "the write-set record of 2000000 bytes is over the largest a round holds, 1048576 bytes"
    );
    assert!(
        malformed()?
            .to_string()
            .starts_with("a write-set record is malformed: "),
        "{}",
        malformed()?
    );
    assert_eq!(
        CommitError::unsealed().to_string(),
        "a read set reached the commit step unsealed"
    );
    assert_eq!(
        CommitError::unavailable().to_string(),
        "the partition refuses writes after a failed log sync, exhausted versions or a close out of order, until it \
         restarts"
    );
    Ok(())
}

#[test]
fn test_only_a_malformed_record_has_a_source() -> TestResult {
    assert!(malformed()?.source().is_some());
    assert!(CommitError::unsealed().source().is_none());
    Ok(())
}

#[test]
fn test_the_operation_refusals_carry_their_codes_and_what_was_found() -> TestResult {
    let failed = CommitError::operation_failed(Some("\"done\"".to_string()));
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
    let nothing = CommitError::operation_failed(None);
    assert_eq!(nothing.found(), None);
    assert_eq!(
        nothing.to_string(),
        "the compare-and-set did not hold: no value is held"
    );
    let refused = CommitError::refused("no".to_string());
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
    assert_eq!(CommitError::unavailable().found(), None);
    Ok(())
}

/// Every reason against `PROTOCOL.md` §9's table, written out by hand: its code's number, its `detail` and whether a
/// client retries it. The table is the independent source; `ErrorCode::retryable` is checked against the same column so
/// the commit step and the wire cannot disagree on what a client may retry.
#[test]
fn test_every_reason_matches_the_wire_tables_code_detail_and_retryability() -> TestResult {
    // (reason, code number, detail, retryable), each row from §9.4: Conflict 2 with the kind's number (Statement is 2,
    // Catalog 5), TooOld 3, Busy 5 with detail 0, TooLarge 6 with the limit, Refused 7, OperationFailed 8,
    // RequestOutOfOrder 13 with the last request the partition holds, SessionBusy 14 with detail 0, retryable after a
    // delay, Unavailable 17, also for a close out of order, NotCommitted 18 with the request, CrossPartition 23 with the
    // transaction's partition in bits 0–31 and the other's in bits 32–63 (1 + 2 × 2^32 = 8,589,934,593); a malformed or
    // unsealed hand-over is a Protocol error, code 1.
    let rows: Vec<(CommitError, u16, u64, bool)> = vec![
        (CommitError::conflict(ConflictKind::Statement), 2, 2, true),
        (CommitError::conflict(ConflictKind::Catalog), 2, 5, true),
        (CommitError::too_old(4, 10), 3, 0, true),
        (CommitError::busy(64), 5, 0, true),
        (
            CommitError::too_large(2_000_000, 1_048_576),
            6,
            1_048_576,
            false,
        ),
        (CommitError::refused("no".to_string()), 7, 0, false),
        (CommitError::operation_failed(None), 8, 0, false),
        (CommitError::request_out_of_order(9), 13, 9, false),
        (CommitError::session_busy(u128::MAX), 14, 0, true),
        (CommitError::unavailable(), 17, 0, false),
        (CommitError::close_out_of_order(9, 11), 17, 0, false),
        (CommitError::not_committed(8), 18, 8, true),
        (CommitError::cross_partition(1, 2), 23, 8_589_934_593, false),
        (
            CommitError::cross_partition(u32::MAX, 0),
            23,
            0xFFFF_FFFF,
            false,
        ),
        (malformed()?, 1, 0, false),
        (CommitError::unsealed(), 1, 0, false),
    ];
    for (reason, number, detail, retryable) in rows {
        assert_eq!(reason.code().number(), number, "{reason:?}");
        assert_eq!(reason.detail(), detail, "{reason:?}");
        assert_eq!(reason.is_retryable(), retryable, "{reason:?}");
        assert_eq!(
            reason.code().retryable(),
            retryable,
            "{reason:?}: the wire's code"
        );
    }
    Ok(())
}

#[test]
fn test_the_reasons_stage_5_5_adds_name_what_failed() {
    let close = CommitError::close_out_of_order(9, 11);
    assert_eq!(
        close.0,
        CommitErrorEnum::CloseOutOfOrder {
            delivered: 9,
            close: 11
        }
    );
    assert_eq!(
        close.to_string(),
        "the close of epoch 11 was delivered after the close of epoch 9: closes arrive one above the last, so the \
         partition stops"
    );
    let cross = CommitError::cross_partition(1, 2);
    assert_eq!(
        cross.0,
        CommitErrorEnum::CrossPartition {
            partition: 1,
            other: 2
        }
    );
    assert_eq!(
        cross.to_string(),
        "the transaction, bound to partition 1, reached partition 2: cross-partition work is refused in this stage"
    );
    assert_eq!(
        CommitError::request_out_of_order(7).0,
        CommitErrorEnum::RequestOutOfOrder { last: 7 }
    );
    assert_eq!(
        CommitError::request_out_of_order(7).to_string(),
        "the request number is below the session's last committed request in this partition, 7"
    );
    assert_eq!(
        CommitError::session_busy(0x2A).0,
        CommitErrorEnum::SessionBusy { session: 0x2A }
    );
    assert_eq!(
        CommitError::session_busy(0x2A).to_string(),
        "session 0000000000000000000000000000002a already has a commit in the commit step"
    );
    assert_eq!(
        CommitError::not_committed(8).to_string(),
        "request 8 was not committed: run its transaction again"
    );
    assert_eq!(
        CommitError::busy(64).to_string(),
        "the commit queue is full at 64 requests, or the partition waits for a close it asked for: retry"
    );
    assert!(close.source().is_none() && cross.source().is_none());
    assert_eq!(cross.found(), None);
    assert_eq!(close.found(), None);
}
