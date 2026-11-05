/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_partition::PartitionError;
use ultragraphdb_common_storage::{error_reply, registration_outcome, scan_refusal};
use ultragraphdb_error_store::StoreError;
use ultragraphdb_protocol_wire::{ConflictKind, ErrorCode};
use ultragraphdb_store_apply::ChangesetError;
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

/// A write past `statements_per_object_max` is code 6, stating the bound.
#[test]
fn test_a_statement_past_its_bound_states_the_bound() {
    let refused = TxnError::changeset(ChangesetError::statements_per_object(16_000));
    assert_eq!(said(&refused.into()), (ErrorCode::TooLarge, 16_000));
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
    assert_eq!(said(&poisoned.into()), (ErrorCode::Unavailable, 0));
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

/// A Commit reply past the floor of `max_frame` is refused as too large, naming the floor.
#[test]
fn test_a_reply_too_large_names_the_floor() {
    assert_eq!(
        said(&CommitError::reply_too_large(600_000, 524_288).into()),
        (ErrorCode::TooLarge, 524_288)
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

/// A store that failed is code 17 through every layer that carries it; a request it refuses as it stands stays code 7.
#[test]
fn test_a_failed_store_is_unavailable_through_every_layer() {
    let failed = StoreError::read_failed(b"k".to_vec(), "a checksum".to_string());
    let cases: [PartitionError; 5] = [
        QuadError::from(failed.clone()).into(),
        QuadError::from(PostingError::from(failed.clone())).into(),
        ChangesetError::from(failed.clone()).into(),
        ChangesetError::from(QuadError::from(failed.clone())).into(),
        TxnError::changeset(ChangesetError::from(failed)).into(),
    ];
    for refusal in &cases {
        assert_eq!(said(refusal), (ErrorCode::Unavailable, 0), "{refusal}");
        assert_eq!(
            registration_outcome(refusal),
            ErrorCode::Unavailable.number()
        );
    }
    let refused: PartitionError =
        QuadError::from(StoreError::key_too_large(9, 1, Vec::new())).into();
    assert_eq!(said(&refused), (ErrorCode::Refused, 0));
    assert_eq!(registration_outcome(&refused), ErrorCode::Refused.number());
}

/// A wrong partition states its bucket in bits 0 to 23 and its owner in bits 32 to 63, and a home's reply says it with outcome 26.
#[test]
fn test_a_wrong_partition_states_its_bucket_and_owner() {
    let refusal = PartitionError::wrong_partition(30_000, 2);
    assert_eq!(
        said(&refusal),
        (ErrorCode::WrongPartition, 30_000 | (2 << 32))
    );
    assert_eq!(
        registration_outcome(&refusal),
        ErrorCode::WrongPartition.number()
    );
}

/// A home's outcome ends its registrant's transaction with code 17 for a failed store, and with code 7 otherwise.
#[test]
fn test_a_homes_outcome_becomes_its_code() {
    let failed = PartitionError::refused(ErrorCode::Unavailable.number());
    assert_eq!(said(&failed), (ErrorCode::Unavailable, 0));
    let refused = PartitionError::refused(ErrorCode::Refused.number());
    assert_eq!(said(&refused), (ErrorCode::Refused, 0));
}

/// A Scan's refusal names the key the store failed under, through every layer, and where the read stood only when the store named
/// none; any other is the partition's own.
#[test]
fn test_a_scans_refusal_names_the_key_of_a_failed_store() {
    let failing = StoreError::read_failed(b"k".to_vec(), "a checksum".to_string());
    let direct: PartitionError = QuadError::from(failing.clone()).into();
    let through_txn: PartitionError =
        TxnError::changeset(ChangesetError::from(QuadError::from(failing))).into();
    for failed in [direct, through_txn] {
        let reply = scan_refusal(&[0xAB, 0x01], &failed);
        assert_eq!(reply.code(), ErrorCode::Unavailable);
        let named = format!("at the key {:02x}:", b'k');
        assert!(reply.message().contains(&named), "{}", reply.message());
    }
    let unnamed: PartitionError = QuadError::from(StoreError::read_failed(
        Vec::new(),
        "a checksum".to_string(),
    ))
    .into();
    let reply = scan_refusal(&[0xAB, 0x01], &unnamed);
    assert!(
        reply.message().contains("at the key ab01:"),
        "{}",
        reply.message()
    );
    let other = TxnError::past_limit(10_000).into();
    assert_eq!(scan_refusal(&[0xAB], &other), error_reply(&other));
}
