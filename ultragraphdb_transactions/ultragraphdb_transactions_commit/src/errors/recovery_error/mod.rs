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
    Log { cause: LogError },
    /// A round the log holds is not a Round body: `position` names it.
    Round { position: u64, cause: ProtocolError },
    /// The store refused a replayed round: `position` names it.
    Apply {
        position: u64,
        cause: ChangesetError,
    },
    /// The store's session table could not be read.
    Sessions { cause: StoreError },
    /// The store's applied version is no round's last version the log holds: the store is ahead of the log, or was
    /// written outside it.
    StoreNotInLog { version: u64 },
    /// The store is empty and the log was trimmed through `checkpoint`: the rounds the checkpoint covers are lost.
    Gap { checkpoint: u64 },
    /// The store's applied version, `applied`, is below `checkpoint`, the last commit version the log's checkpoint
    /// covers: rounds the store lost are no longer in the log, and nothing is replayed.
    BehindCheckpoint { applied: u64, checkpoint: u64 },
}

impl RecoveryError {
    #[allow(non_snake_case)]
    pub fn Log(cause: LogError) -> Self {
        Self(RecoveryErrorEnum::Log { cause })
    }

    #[allow(non_snake_case)]
    pub fn Round(position: u64, cause: ProtocolError) -> Self {
        Self(RecoveryErrorEnum::Round { position, cause })
    }

    #[allow(non_snake_case)]
    pub fn Apply(position: u64, cause: ChangesetError) -> Self {
        Self(RecoveryErrorEnum::Apply { position, cause })
    }

    #[allow(non_snake_case)]
    pub fn Sessions(cause: StoreError) -> Self {
        Self(RecoveryErrorEnum::Sessions { cause })
    }

    #[allow(non_snake_case)]
    pub fn StoreNotInLog(version: u64) -> Self {
        Self(RecoveryErrorEnum::StoreNotInLog { version })
    }

    #[allow(non_snake_case)]
    pub fn Gap(checkpoint: u64) -> Self {
        Self(RecoveryErrorEnum::Gap { checkpoint })
    }

    #[allow(non_snake_case)]
    pub fn BehindCheckpoint(applied: u64, checkpoint: u64) -> Self {
        Self(RecoveryErrorEnum::BehindCheckpoint {
            applied,
            checkpoint,
        })
    }
}
