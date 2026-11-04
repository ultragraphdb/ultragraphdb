/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_partition::{PartitionError, PartitionErrorEnum};
use ultragraphdb_protocol_wire::{ErrorCode, ErrorReply};
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
        PartitionErrorEnum::Refused { .. }
        | PartitionErrorEnum::Changeset(_)
        | PartitionErrorEnum::Quad(_) => (ErrorCode::Refused, 0),
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
        TxnErrorEnum::PastLimit { .. } | TxnErrorEnum::Ended => (ErrorCode::TimedOut, 0),
        TxnErrorEnum::WriteSetTooLarge { .. } | TxnErrorEnum::ReadSetTooLarge { .. } => {
            (ErrorCode::TooLarge, 0)
        }
        TxnErrorEnum::ReadOnly
        | TxnErrorEnum::Namespace { .. }
        | TxnErrorEnum::Changeset(_)
        | TxnErrorEnum::Declaration => (ErrorCode::Refused, 0),
    }
}

fn commit_code(error: &CommitError) -> (ErrorCode, u64) {
    match &error.0 {
        CommitErrorEnum::Conflict { .. } => (ErrorCode::Conflict, 0),
        CommitErrorEnum::TooOld { .. } => (ErrorCode::TooOld, 0),
        CommitErrorEnum::TooLarge { .. } => (ErrorCode::TooLarge, 0),
        CommitErrorEnum::Busy { .. } => (ErrorCode::Busy, 0),
        CommitErrorEnum::RequestOutOfOrder { last, .. } => (ErrorCode::RequestOutOfOrder, *last),
        CommitErrorEnum::SessionBusy { .. } => (ErrorCode::SessionBusy, 0),
        CommitErrorEnum::NotCommitted { .. } => (ErrorCode::NotCommitted, 0),
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
