/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_store_apply::ChangesetError;
use ultragraphdb_transactions_txn::{TxnError, TxnErrorEnum};

fn changeset_error() -> ChangesetError {
    ChangesetError::Rebuilding("http://ex.org/p")
}

#[test]
fn test_each_constructor_builds_its_variant() {
    assert_eq!(
        TxnError::ReadAboveWatermark(12, 10).0,
        TxnErrorEnum::ReadAboveWatermark {
            version: 12,
            watermark: 10
        }
    );
    assert_eq!(
        TxnError::PastLimit(10_000).0,
        TxnErrorEnum::PastLimit { limit_ms: 10_000 }
    );
    assert_eq!(
        TxnError::WriteSetTooLarge(5, 4).0,
        TxnErrorEnum::WriteSetTooLarge { size: 5, limit: 4 }
    );
    assert_eq!(
        TxnError::ReadSetTooLarge(7, 6).0,
        TxnErrorEnum::ReadSetTooLarge { size: 7, limit: 6 }
    );
    assert_eq!(TxnError::ReadOnly().0, TxnErrorEnum::ReadOnly);
    assert_eq!(
        TxnError::Namespace(0, 3).0,
        TxnErrorEnum::Namespace {
            transaction: 0,
            changeset: 3
        }
    );
    assert_eq!(TxnError::Ended().0, TxnErrorEnum::Ended);
    assert_eq!(
        TxnError::Changeset(changeset_error()).0,
        TxnErrorEnum::Changeset(changeset_error())
    );
    assert_eq!(
        TxnError::from(changeset_error()),
        TxnError::Changeset(changeset_error())
    );
}

#[test]
fn test_only_past_its_limit_is_retryable() {
    assert!(TxnError::PastLimit(16_000).is_retryable());
    for refused in [
        TxnError::ReadAboveWatermark(12, 10),
        TxnError::WriteSetTooLarge(5, 4),
        TxnError::ReadSetTooLarge(7, 6),
        TxnError::ReadOnly(),
        TxnError::Namespace(0, 3),
        TxnError::Ended(),
        TxnError::Changeset(changeset_error()),
    ] {
        assert!(!refused.is_retryable(), "{refused:?}");
    }
}

#[test]
fn test_each_text_names_what_was_refused() {
    // The texts are this crate's own: no Dgraph text exists for them.
    assert_eq!(
        TxnError::ReadAboveWatermark(12, 10).to_string(),
        "read version 12 is above the visibility watermark 10"
    );
    assert_eq!(
        TxnError::PastLimit(10_000).to_string(),
        "transaction past its limit of 10 s"
    );
    assert_eq!(
        TxnError::PastLimit(16_000).to_string(),
        "transaction past its limit of 16 s"
    );
    assert_eq!(
        TxnError::PastLimit(1_500).to_string(),
        "transaction past its limit of 1500 ms"
    );
    assert_eq!(
        TxnError::WriteSetTooLarge(5, 4).to_string(),
        "write set of 5 bytes exceeds the limit of 4 bytes"
    );
    assert_eq!(
        TxnError::ReadSetTooLarge(7, 6).to_string(),
        "read set of 7 bytes exceeds the limit of 6 bytes"
    );
    assert_eq!(
        TxnError::ReadOnly().to_string(),
        "a read-only transaction does not write"
    );
    assert_eq!(
        TxnError::Namespace(0, 3).to_string(),
        "a changeset of namespace 3 in a transaction of namespace 0"
    );
    assert_eq!(TxnError::Ended().to_string(), "the transaction has ended");
    assert_eq!(
        TxnError::Changeset(changeset_error()).to_string(),
        changeset_error().to_string()
    );
}

#[test]
fn test_a_changesets_error_is_the_source() {
    assert!(TxnError::Changeset(changeset_error()).source().is_some());
    assert!(TxnError::Ended().source().is_none());
}
