/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_partition::PartitionError;
use ultragraphdb_common_storage::error_reply;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_protocol_wire::{ConflictKind, ErrorCode};
use ultragraphdb_store_posting::PostingError;
use ultragraphdb_store_quads::QuadError;
use ultragraphdb_transactions_commit::CommitError;
use ultragraphdb_transactions_txn::TxnError;

fn said(error: &PartitionError) -> (ErrorCode, u64) {
    let reply = error_reply(error);
    (reply.code(), reply.detail())
}

/// A read past the read set's room states the limit in bytes.
#[test]
fn test_a_read_set_too_large_states_its_limit() {
    let refused = TxnError::read_set_too_large(9_000, 1_024);
    assert_eq!(said(&refused.into()), (ErrorCode::TooLarge, 1_024));
}

/// A commit's write set past a round's bound states that bound in bytes.
#[test]
fn test_a_commit_too_large_states_its_limit() {
    let refused = CommitError::too_large(9_000, 1_024);
    assert_eq!(said(&refused.into()), (ErrorCode::TooLarge, 1_024));
}

/// A transaction past its limit is code 4, stating the limit.
#[test]
fn test_a_transaction_past_its_limit_states_it() {
    assert_eq!(
        said(&TxnError::past_limit(10_000).into()),
        (ErrorCode::PastLimit, 10_000)
    );
}

/// A request of a transaction already ended finds a stream that holds none.
#[test]
fn test_an_ended_transaction_is_refused_as_not_held() {
    assert_eq!(said(&TxnError::ended().into()), (ErrorCode::Protocol, 0));
}

/// A read below what the partition retains is code 3, whichever layer refused it.
#[test]
fn test_a_read_below_what_is_retained_is_too_old() {
    let cases: [QuadError; 2] = [
        PostingError::ts_too_old().into(),
        StoreError::ts_too_old().into(),
    ];
    for case in cases {
        assert_eq!(said(&case.into()), (ErrorCode::TooOld, 0));
    }
    let store: PostingError = StoreError::ts_too_old().into();
    assert_eq!(said(&QuadError::from(store).into()), (ErrorCode::TooOld, 0));
    let poisoned: QuadError = StoreError::poisoned().into();
    assert_eq!(said(&poisoned.into()), (ErrorCode::Refused, 0));
}

/// A Bind repeating an identity the partition holds is a protocol error.
#[test]
fn test_a_held_identity_is_a_protocol_error() {
    assert_eq!(
        said(&PartitionError::transaction_held([7; 16])),
        (ErrorCode::Protocol, 0)
    );
}

/// What a Recall refuses with: its request's state in the session table, each by its code and detail.
#[test]
fn test_a_recall_refused_says_why() {
    assert_eq!(
        said(&CommitError::session_busy(1).into()),
        (ErrorCode::SessionBusy, 0)
    );
    assert_eq!(
        said(&CommitError::request_out_of_order(9).into()),
        (ErrorCode::RequestOutOfOrder, 9)
    );
    assert_eq!(
        said(&CommitError::not_committed(5).into()),
        (ErrorCode::NotCommitted, 5)
    );
}

/// A conflict states its kind.
#[test]
fn test_a_conflict_states_its_kind() {
    for kind in ConflictKind::ALL {
        assert_eq!(
            said(&CommitError::conflict(kind).into()),
            (ErrorCode::Conflict, kind.number())
        );
    }
}
