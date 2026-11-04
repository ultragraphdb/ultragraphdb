/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_apply::ChangesetError;
use ultragraphdb_store_quads::QuadError;
use ultragraphdb_transactions_commit::{CommitError, PipelineError};
use ultragraphdb_transactions_txn::TxnError;

use crate::errors::partition_error::{PartitionError, PartitionErrorEnum};

impl From<TxnError> for PartitionError {
    fn from(e: TxnError) -> Self {
        Self(PartitionErrorEnum::Txn(e))
    }
}

impl From<ChangesetError> for PartitionError {
    fn from(e: ChangesetError) -> Self {
        Self(PartitionErrorEnum::Changeset(e))
    }
}

impl From<QuadError> for PartitionError {
    fn from(e: QuadError) -> Self {
        Self(PartitionErrorEnum::Quad(e))
    }
}

impl From<PipelineError> for PartitionError {
    fn from(e: PipelineError) -> Self {
        Self(PartitionErrorEnum::Pipeline(e))
    }
}

impl From<CommitError> for PartitionError {
    fn from(e: CommitError) -> Self {
        Self(PartitionErrorEnum::Commit(e))
    }
}
