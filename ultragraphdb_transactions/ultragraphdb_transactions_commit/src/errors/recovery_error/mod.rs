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
    /// The store's applied log position could not be read: the store's error, or its corruption error for a value that
    /// is not 8 bytes.
    AppliedPosition {
        /// The store's error.
        cause: StoreError,
    },
    /// The store's applied log position lies beyond the log's tail: the store is ahead of the log, or was written
    /// outside it.
    StoreNotInLog {
        /// The store's applied log position.
        position: u64,
    },
    /// The log has a gap between the store's applied position and its tail: `position` is the first missing.
    Gap {
        /// The first position missing.
        position: u64,
    },
    /// The store's applied log position, `applied`, is below `checkpoint`, the log's checkpoint: the rounds between
    /// them are trimmed, and nothing is replayed.
    BehindCheckpoint {
        /// The store's applied log position.
        applied: u64,
        /// The log's checkpoint position.
        checkpoint: u64,
    },
    /// The round at `position` names `derivation`, a derivation version the store does not know: its quads cannot be
    /// turned into keys, and nothing of it is applied.
    Derivation {
        /// The log position of the round.
        position: u64,
        /// The derivation version it names.
        derivation: u32,
    },
    /// The round at `position` starts version `version` of `tenant`'s catalog, which the metadata group could not
    /// give or which does not decode, as `reason` says: nothing of the round is applied.
    Catalog {
        /// The log position of the round.
        position: u64,
        /// The tenant whose catalog version it names.
        tenant: u64,
        /// The catalog version it names.
        version: u64,
        /// Why the version could not be read.
        reason: String,
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
    pub fn store_not_in_log(position: u64) -> Self {
        Self(RecoveryErrorEnum::StoreNotInLog { position })
    }

    /// The [`RecoveryErrorEnum::Gap`] reason.
    pub fn gap(position: u64) -> Self {
        Self(RecoveryErrorEnum::Gap { position })
    }

    /// The [`RecoveryErrorEnum::BehindCheckpoint`] reason.
    pub fn behind_checkpoint(applied: u64, checkpoint: u64) -> Self {
        Self(RecoveryErrorEnum::BehindCheckpoint {
            applied,
            checkpoint,
        })
    }

    /// The [`RecoveryErrorEnum::Derivation`] reason.
    pub fn derivation(position: u64, derivation: u32) -> Self {
        Self(RecoveryErrorEnum::Derivation {
            position,
            derivation,
        })
    }

    /// The [`RecoveryErrorEnum::Catalog`] reason.
    pub fn catalog(position: u64, tenant: u64, version: u64, reason: impl Into<String>) -> Self {
        Self(RecoveryErrorEnum::Catalog {
            position,
            tenant,
            version,
            reason: reason.into(),
        })
    }

    /// The [`RecoveryErrorEnum::AppliedPosition`] reason.
    pub fn applied_position(cause: StoreError) -> Self {
        Self(RecoveryErrorEnum::AppliedPosition { cause })
    }
}
