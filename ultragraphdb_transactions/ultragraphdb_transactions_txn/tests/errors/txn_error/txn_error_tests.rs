/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;
use ultragraphdb_tool_bdd::{Context, TestResult};

use ultragraphdb_store_apply::ChangesetError;
use ultragraphdb_transactions_txn::{TxnError, TxnErrorEnum};

fn changeset_error() -> ChangesetError {
    ChangesetError::rebuilding("http://ex.org/p")
}

#[test]
fn test_each_constructor_builds_its_variant() {
    assert_eq!(
        TxnError::read_above_watermark(12, 10).0,
        TxnErrorEnum::ReadAboveWatermark {
            version: 12,
            watermark: 10
        }
    );
    assert_eq!(
        TxnError::past_limit(10_000).0,
        TxnErrorEnum::PastLimit { limit_ms: 10_000 }
    );
    assert_eq!(
        TxnError::write_set_too_large(5, 4).0,
        TxnErrorEnum::WriteSetTooLarge { size: 5, limit: 4 }
    );
    assert_eq!(
        TxnError::read_set_too_large(7, 6).0,
        TxnErrorEnum::ReadSetTooLarge { size: 7, limit: 6 }
    );
    assert_eq!(TxnError::read_only().0, TxnErrorEnum::ReadOnly);
    assert_eq!(
        TxnError::namespace(0, 3).0,
        TxnErrorEnum::Namespace {
            transaction: 0,
            changeset: 3
        }
    );
    assert_eq!(TxnError::ended().0, TxnErrorEnum::Ended);
    assert_eq!(
        TxnError::changeset(changeset_error()).0,
        TxnErrorEnum::Changeset(changeset_error())
    );
    assert_eq!(
        TxnError::from(changeset_error()),
        TxnError::changeset(changeset_error())
    );
    assert_eq!(
        TxnError::cross_partition(1, 2).0,
        TxnErrorEnum::CrossPartition {
            partition: 1,
            other: 2
        }
    );
    assert_eq!(TxnError::declaration().0, TxnErrorEnum::Declaration);
}

/// Each refusal answers the wire's code its doc names: past its limit, too large for either set, cross-partition, and
/// refused for every other.
#[test]
fn test_each_refusal_answers_its_wire_code() {
    use ultragraphdb_protocol_wire::ErrorCode;
    let cases = [
        (TxnError::past_limit(10_000), ErrorCode::PastLimit),
        (TxnError::write_set_too_large(5, 4), ErrorCode::TooLarge),
        (TxnError::read_set_too_large(7, 6), ErrorCode::TooLarge),
        (TxnError::cross_partition(1, 2), ErrorCode::CrossPartition),
        (TxnError::read_above_watermark(12, 10), ErrorCode::Refused),
        (TxnError::read_only(), ErrorCode::Refused),
        (TxnError::namespace(0, 3), ErrorCode::Refused),
        (TxnError::ended(), ErrorCode::Refused),
        (TxnError::changeset(changeset_error()), ErrorCode::Refused),
        (TxnError::declaration(), ErrorCode::Refused),
    ];
    for (error, code) in cases {
        assert_eq!(error.code(), code, "{error:?}");
    }
}

#[test]
fn test_only_past_its_limit_is_retryable() {
    assert!(TxnError::past_limit(16_000).is_retryable());
    for refused in [
        TxnError::read_above_watermark(12, 10),
        TxnError::write_set_too_large(5, 4),
        TxnError::read_set_too_large(7, 6),
        TxnError::read_only(),
        TxnError::namespace(0, 3),
        TxnError::ended(),
        TxnError::changeset(changeset_error()),
        TxnError::cross_partition(1, 2),
        TxnError::declaration(),
    ] {
        assert!(!refused.is_retryable(), "{refused:?}");
    }
}

#[test]
fn test_each_text_names_what_was_refused() {
    // The texts are this crate's own: no Dgraph text exists for them.
    assert_eq!(
        TxnError::read_above_watermark(12, 10).to_string(),
        "read version 12 is above the visibility watermark 10"
    );
    assert_eq!(
        TxnError::past_limit(10_000).to_string(),
        "transaction past its limit of 10 s"
    );
    assert_eq!(
        TxnError::past_limit(16_000).to_string(),
        "transaction past its limit of 16 s"
    );
    assert_eq!(
        TxnError::past_limit(1_500).to_string(),
        "transaction past its limit of 1500 ms"
    );
    assert_eq!(
        TxnError::write_set_too_large(5, 4).to_string(),
        "write set of 5 bytes exceeds the limit of 4 bytes"
    );
    assert_eq!(
        TxnError::read_set_too_large(7, 6).to_string(),
        "read set of 7 bytes exceeds the limit of 6 bytes"
    );
    assert_eq!(
        TxnError::read_only().to_string(),
        "a read-only transaction does not write"
    );
    assert_eq!(
        TxnError::namespace(0, 3).to_string(),
        "a changeset of namespace 3 in a transaction of namespace 0"
    );
    assert_eq!(TxnError::ended().to_string(), "the transaction has ended");
    // A changeset's refusal keeps the applier's own text, Dgraph's retry text for a declaration during a build.
    assert_eq!(
        TxnError::changeset(changeset_error()).to_string(),
        "An index is being built, so <http://ex.org/p> cannot be declared now. Please retry"
    );
}

#[test]
fn test_a_changesets_error_is_the_source() -> TestResult {
    let error = TxnError::changeset(changeset_error());
    let source = error.source().context("a source")?;
    assert_eq!(
        source.downcast_ref::<ChangesetError>(),
        Some(&changeset_error()),
        "the source is the changeset's error itself"
    );
    for refused in [
        TxnError::read_above_watermark(12, 10),
        TxnError::past_limit(10_000),
        TxnError::write_set_too_large(5, 4),
        TxnError::read_set_too_large(7, 6),
        TxnError::read_only(),
        TxnError::namespace(0, 3),
        TxnError::ended(),
        TxnError::declaration(),
    ] {
        assert!(refused.source().is_none(), "{refused:?}");
    }
    Ok(())
}

#[test]
fn test_a_cross_partition_abort_names_both_partitions_and_is_not_retryable() {
    let error = TxnError::cross_partition(1, 2);
    assert_eq!(
        error.0,
        TxnErrorEnum::CrossPartition {
            partition: 1,
            other: 2
        }
    );
    assert!(!error.is_retryable());
    assert_eq!(
        error.to_string(),
        "the transaction, bound to partition 1, reached partition 2: cross-partition work is refused in this stage"
    );
    assert!(error.source().is_none());
}
