/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod recovery_error_display;
mod recovery_error_error;

use ultragraphdb_error_log::LogError;
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_apply::ChangesetError;

/// Why recovery refused to start the node, or could not replay the log into the store (7.9). The node does not start.
#[derive(Debug, Clone, PartialEq)]
pub struct RecoveryError(pub RecoveryErrorEnum);

/// The reasons.
#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryErrorEnum {
    /// The log could not be read.
    Log {
        /// The log's error.
        cause: LogError,
    },
    /// A round the log holds is not a Round body: `position` names it.
    Round {
        /// The log position of the round.
        position: u64,
        /// Why its body did not read.
        cause: ProtocolError,
    },
    /// The store refused a replayed round: `position` names it.
    Apply {
        /// The log position of the round.
        position: u64,
        /// The store's refusal of the round's changeset.
        cause: ChangesetError,
    },
    /// The store's session table could not be read.
    Sessions {
        /// The store's error reading the session table.
        cause: StoreError,
    },
    /// The store's applied version is no round's last version the log holds: the store is ahead of the log, or was
    /// written outside it.
    StoreNotInLog {
        /// The store's applied version.
        version: u64,
    },
    /// The store is empty and the log was trimmed through `checkpoint`: the rounds the checkpoint covers are lost.
    Gap {
        /// The last commit version the log's checkpoint covers.
        checkpoint: u64,
    },
    /// The store's applied version, `applied`, is below `checkpoint`, the last commit version the log's checkpoint
    /// covers: rounds the store lost are no longer in the log, and nothing is replayed.
    BehindCheckpoint {
        /// The store's applied version.
        applied: u64,
        /// The last commit version the log's checkpoint covers.
        checkpoint: u64,
    },
}

impl RecoveryError {
    /// The [`RecoveryErrorEnum::Log`] reason.
    pub fn log(cause: LogError) -> Self {
        Self(RecoveryErrorEnum::Log { cause })
    }

    /// The [`RecoveryErrorEnum::Round`] reason.
    pub fn round(position: u64, cause: ProtocolError) -> Self {
        Self(RecoveryErrorEnum::Round { position, cause })
    }

    /// The [`RecoveryErrorEnum::Apply`] reason.
    pub fn apply(position: u64, cause: ChangesetError) -> Self {
        Self(RecoveryErrorEnum::Apply { position, cause })
    }

    /// The [`RecoveryErrorEnum::Sessions`] reason.
    pub fn sessions(cause: StoreError) -> Self {
        Self(RecoveryErrorEnum::Sessions { cause })
    }

    /// The [`RecoveryErrorEnum::StoreNotInLog`] reason.
    pub fn store_not_in_log(version: u64) -> Self {
        Self(RecoveryErrorEnum::StoreNotInLog { version })
    }

    /// The [`RecoveryErrorEnum::Gap`] reason.
    pub fn gap(checkpoint: u64) -> Self {
        Self(RecoveryErrorEnum::Gap { checkpoint })
    }

    /// The [`RecoveryErrorEnum::BehindCheckpoint`] reason.
    pub fn behind_checkpoint(applied: u64, checkpoint: u64) -> Self {
        Self(RecoveryErrorEnum::BehindCheckpoint {
            applied,
            checkpoint,
        })
    }
}
