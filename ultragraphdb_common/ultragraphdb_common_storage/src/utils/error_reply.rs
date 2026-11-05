/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_partition::{PartitionError, PartitionErrorEnum};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply};
use ultragraphdb_store_apply::{ChangesetError, ChangesetErrorEnum};
use ultragraphdb_store_posting::{PostingError, PostingErrorEnum};
use ultragraphdb_store_quads::{QuadError, QuadErrorEnum};
use ultragraphdb_transactions_commit::{CommitError, CommitErrorEnum};
use ultragraphdb_transactions_txn::{TxnError, TxnErrorEnum};

/// What a refusal of the partition says on the wire (`PROTOCOL.md` §10), its message the refusal's own.
#[must_use]
pub fn error_reply(error: &PartitionError) -> ErrorReply {
    let message = error.to_string();
    let (code, detail) = match &error.0 {
        PartitionErrorEnum::WrongPartition { bucket, .. } => {
            (ErrorCode::WrongPartition, u64::from(*bucket))
        }
        PartitionErrorEnum::CrossPartition { partition, other } => (
            ErrorCode::CrossPartition,
            u64::from(*partition) | (u64::from(*other) << 32),
        ),
        PartitionErrorEnum::TimedOut { partition } => (ErrorCode::TimedOut, u64::from(*partition)),
        PartitionErrorEnum::Refused { .. } => (ErrorCode::Refused, 0),
        PartitionErrorEnum::TransactionHeld { .. } => (ErrorCode::Protocol, 0),
        PartitionErrorEnum::Changeset(refusal) => changeset_code(refusal),
        PartitionErrorEnum::Quad(refusal) => quad_code(refusal),
        PartitionErrorEnum::Pipeline(_) => (ErrorCode::Unavailable, 0),
        PartitionErrorEnum::Txn(refusal) => txn_code(refusal),
        PartitionErrorEnum::Commit(refusal) => commit_code(refusal),
    };
    ErrorReply::new(code, message).with_detail(detail)
}

fn txn_code(error: &TxnError) -> (ErrorCode, u64) {
    match &error.0 {
        TxnErrorEnum::ReadAboveWatermark { watermark, .. } => {
            (ErrorCode::AheadOfWatermark, *watermark)
        }
        TxnErrorEnum::CrossPartition { partition, other } => (
            ErrorCode::CrossPartition,
            u64::from(*partition) | (u64::from(*other) << 32),
        ),
        TxnErrorEnum::PastLimit { limit_ms } => (ErrorCode::PastLimit, *limit_ms),
        TxnErrorEnum::Ended => (ErrorCode::Protocol, 0),
        TxnErrorEnum::WriteSetTooLarge { limit, .. }
        | TxnErrorEnum::ReadSetTooLarge { limit, .. } => (ErrorCode::TooLarge, *limit as u64),
        TxnErrorEnum::Changeset(refusal) => changeset_code(refusal),
        TxnErrorEnum::ReadOnly | TxnErrorEnum::Tenant { .. } | TxnErrorEnum::Declaration => {
            (ErrorCode::Refused, 0)
        }
    }
}

/// A read below what the partition retains is code 3 (`PROTOCOL.md` §11.9); any other refusal of a changeset, code 7.
fn changeset_code(error: &ChangesetError) -> (ErrorCode, u64) {
    if let ChangesetErrorEnum::Quad(quad) = &error.0 {
        return quad_code(quad);
    }
    refused_or_too_old(
        matches!(&error.0, ChangesetErrorEnum::Posting(posting) if posting_too_old(posting))
            || matches!(&error.0, ChangesetErrorEnum::Store(store) if store_too_old(store)),
    )
}

fn quad_code(error: &QuadError) -> (ErrorCode, u64) {
    refused_or_too_old(
        matches!(&error.0, QuadErrorEnum::Posting(posting) if posting_too_old(posting))
            || matches!(&error.0, QuadErrorEnum::Store(store) if store_too_old(store)),
    )
}

fn refused_or_too_old(too_old: bool) -> (ErrorCode, u64) {
    if too_old {
        (ErrorCode::TooOld, 0)
    } else {
        (ErrorCode::Refused, 0)
    }
}

fn posting_too_old(error: &PostingError) -> bool {
    matches!(
        &error.0,
        PostingErrorEnum::TsTooOld | PostingErrorEnum::ReadBelowMinTs { .. }
    ) || matches!(&error.0, PostingErrorEnum::Store(store) if store_too_old(store))
}

fn store_too_old(error: &StoreError) -> bool {
    matches!(error.0, StoreErrorEnum::TsTooOld)
}

fn commit_code(error: &CommitError) -> (ErrorCode, u64) {
    match &error.0 {
        CommitErrorEnum::Conflict { kind } => (ErrorCode::Conflict, kind.number()),
        CommitErrorEnum::TooOld { .. } => (ErrorCode::TooOld, 0),
        CommitErrorEnum::TooLarge { limit, .. } => (ErrorCode::TooLarge, *limit as u64),
        CommitErrorEnum::Busy { .. } => (ErrorCode::Busy, 0),
        CommitErrorEnum::RequestOutOfOrder { last, .. } => (ErrorCode::RequestOutOfOrder, *last),
        CommitErrorEnum::SessionBusy { .. } => (ErrorCode::SessionBusy, 0),
        CommitErrorEnum::NotCommitted { request } => (ErrorCode::NotCommitted, *request),
        CommitErrorEnum::OperationFailed { .. } => (ErrorCode::OperationFailed, 0),
        CommitErrorEnum::Refused { .. } => (ErrorCode::Refused, 0),
        CommitErrorEnum::CrossPartition { partition, other } => (
            ErrorCode::CrossPartition,
            u64::from(*partition) | (u64::from(*other) << 32),
        ),
        CommitErrorEnum::Malformed { .. }
        | CommitErrorEnum::Unsealed
        | CommitErrorEnum::Unavailable
        | CommitErrorEnum::CloseOutOfOrder { .. } => (ErrorCode::Unavailable, 0),
    }
}
