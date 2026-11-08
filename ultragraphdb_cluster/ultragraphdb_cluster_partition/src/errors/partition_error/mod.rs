/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod partition_error_display;
mod partition_error_error;
mod partition_error_from;

use ultragraphdb_protocol_wire::ErrorCode;
use ultragraphdb_store_apply::{ChangesetError, ChangesetErrorEnum};
use ultragraphdb_store_quads::{QuadError, QuadErrorEnum};
use ultragraphdb_transactions_commit::{CommitError, PipelineError};
use ultragraphdb_transactions_txn::TxnError;

/// Why a partition refused a request, by what the sender must do:
///
/// - **Send it elsewhere:** [`WrongPartition`](PartitionErrorEnum::WrongPartition): the request's bucket is another
///   partition's, which the error names, so a sender with a stale placement map learns the owner.
/// - **Not in this stage:** [`CrossPartition`](PartitionErrorEnum::CrossPartition): the request reaches several
///   partitions, which stage 8 executes through the sequencer.
/// - **Wait or give up:** [`TimedOut`](PartitionErrorEnum::TimedOut): a snapshot read reached its deadline waiting on a
///   partition that has not closed its epoch.
/// - **Refused by the home:** [`Refused`](PartitionErrorEnum::Refused): a registration's home answered it with an error
///   code.
/// - **A broken frontend:** [`TransactionHeld`](PartitionErrorEnum::TransactionHeld): a Bind repeating a transaction's identity.
/// - **Below:** [`Txn`](PartitionErrorEnum::Txn), [`Changeset`](PartitionErrorEnum::Changeset),
///   [`Quad`](PartitionErrorEnum::Quad), [`Pipeline`](PartitionErrorEnum::Pipeline),
///   [`Commit`](PartitionErrorEnum::Commit): the transaction's, the applier's, the store's, the commit pipeline's or
///   the commit step's own error.
#[derive(Debug, Clone, PartialEq)]
pub enum PartitionErrorEnum {
    /// Bucket `bucket` is partition `owner`'s, not the partition's that was asked.
    WrongPartition {
        /// The bucket.
        bucket: u32,
        /// The partition owning it at the request's epoch.
        owner: u32,
    },
    /// The request of partition `partition` reaches partition `other` too: the lowest other partition it reaches.
    CrossPartition {
        /// The partition asked.
        partition: u32,
        /// The lowest other partition the request reaches.
        other: u32,
    },
    /// Partition `partition`, which a snapshot read must ask, has not closed the read's epoch: the read waits for it,
    /// and fails naming it at its deadline, never answering without its rows.
    TimedOut {
        /// The partition waited on.
        partition: u32,
    },
    /// The home answered a registration with the error code `outcome` (`PROTOCOL.md` §15).
    Refused {
        /// The error code.
        outcome: u16,
    },
    /// A Bind named the transaction `identity`, which the partition already holds.
    TransactionHeld {
        /// The identity the Bind named.
        identity: [u8; 16],
    },
    /// The transaction's error.
    Txn(TxnError),
    /// The changeset applier's error.
    Changeset(ChangesetError),
    /// The quad store's error.
    Quad(QuadError),
    /// The commit pipeline's error.
    Pipeline(PipelineError),
    /// The commit step's refusal of an entry.
    Commit(CommitError),
}

/// See [`PartitionErrorEnum`].
#[derive(Debug, Clone, PartialEq)]
pub struct PartitionError(pub PartitionErrorEnum);

impl PartitionError {
    /// [`PartitionErrorEnum::WrongPartition`] for `bucket`, owned by `owner`.
    #[must_use]
    pub fn wrong_partition(bucket: u32, owner: u32) -> Self {
        Self(PartitionErrorEnum::WrongPartition { bucket, owner })
    }

    /// [`PartitionErrorEnum::CrossPartition`] for a request of `partition` reaching `other`.
    #[must_use]
    pub fn cross_partition(partition: u32, other: u32) -> Self {
        Self(PartitionErrorEnum::CrossPartition { partition, other })
    }

    /// [`PartitionErrorEnum::TimedOut`] waiting on `partition`.
    #[must_use]
    pub fn timed_out(partition: u32) -> Self {
        Self(PartitionErrorEnum::TimedOut { partition })
    }

    /// Whether a home's refusal says its dictionary does not hold the skolem IRI it was asked: its dictionary's own refusal, or
    /// from another process the outcome a registration of one carries (`PROTOCOL.md` §13.15).
    #[must_use]
    pub fn holds_no_skolem(&self) -> bool {
        matches!(&self.0, PartitionErrorEnum::Refused { outcome } if *outcome == ErrorCode::Refused.number())
            || matches!(
                &self.0,
                PartitionErrorEnum::Changeset(ChangesetError(ChangesetErrorEnum::Quad(QuadError(
                    QuadErrorEnum::UnknownSkolem { .. }
                ))))
            )
    }

    /// [`PartitionErrorEnum::Refused`] with `outcome`.
    #[must_use]
    pub fn refused(outcome: u16) -> Self {
        Self(PartitionErrorEnum::Refused { outcome })
    }

    /// [`PartitionErrorEnum::TransactionHeld`] for `identity`.
    #[must_use]
    pub fn transaction_held(identity: [u8; 16]) -> Self {
        Self(PartitionErrorEnum::TransactionHeld { identity })
    }
}
