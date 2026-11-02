/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod pipeline_error_display;
mod pipeline_error_error;

use ultragraphdb_error_log::LogError;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_apply::ChangesetError;
use ultragraphdb_transactions_txn::TxnError;

/// Why the pipeline stopped or did not open: the log refused a round or failed its sync, or the store refused a synced
/// round, each of which stops the commit step, so the node refuses writes until it restarts and recovers; or the
/// store's session table could not be read when it opened.
#[derive(Debug, Clone, PartialEq)]
pub struct PipelineError(pub PipelineErrorEnum);

/// The reasons.
#[derive(Debug, Clone, PartialEq)]
pub enum PipelineErrorEnum {
    /// The log refused the round the step closed, or failed to make it durable: none of its transactions is
    /// acknowledged.
    Log {
        /// The log's error.
        cause: LogError,
    },
    /// The store refused to write a round the log holds durably: its transactions are not acknowledged, and recovery
    /// replays them.
    Apply {
        /// The store's refusal of the round's changeset.
        cause: ChangesetError,
    },
    /// The store's session table could not be read, or is corrupted: the pipeline does not open.
    Sessions {
        /// The store's error reading the session table.
        cause: StoreError,
    },
    /// A build's end could not be proposed: the store failed reading the build's catalog. The pipeline stops, and the
    /// build is finished when the store opens again.
    BuildEnd {
        /// The error reading the build's catalog.
        cause: TxnError,
    },
}

impl PipelineError {
    /// The [`PipelineErrorEnum::Log`] reason.
    pub fn log(cause: LogError) -> Self {
        Self(PipelineErrorEnum::Log { cause })
    }

    /// The [`PipelineErrorEnum::Apply`] reason.
    pub fn apply(cause: ChangesetError) -> Self {
        Self(PipelineErrorEnum::Apply { cause })
    }

    /// The [`PipelineErrorEnum::Sessions`] reason.
    pub fn sessions(cause: StoreError) -> Self {
        Self(PipelineErrorEnum::Sessions { cause })
    }

    /// The [`PipelineErrorEnum::BuildEnd`] reason.
    pub fn build_end(cause: TxnError) -> Self {
        Self(PipelineErrorEnum::BuildEnd { cause })
    }
}
