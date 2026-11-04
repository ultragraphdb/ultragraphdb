/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod config_error_display;
mod config_error_error;

use ultragraphdb_common_config_check::Defect;
use ultragraphdb_error_log::LogError;

/// Why the config service refused an operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(pub ConfigErrorEnum);

/// Each reason, one per thing a caller must do differently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigErrorEnum {
    /// No revision of the target exists.
    UnknownTarget {
        /// The target asked for.
        target: String,
    },
    /// The target was retired: it serves no instance.
    Retired {
        /// The target.
        target: String,
    },
    /// The target's revisions exist and none is active.
    NoActiveRevision {
        /// The target.
        target: String,
    },
    /// The target holds no such revision.
    UnknownRevision {
        /// The target.
        target: String,
        /// The revision asked for.
        revision: u64,
    },
    /// The revision holds no such instance.
    UnknownInstance {
        /// The target.
        target: String,
        /// The revision.
        revision: u64,
        /// The instance asked for.
        instance: String,
    },
    /// The revision was edited from one no longer the latest.
    Stale {
        /// The target.
        target: String,
        /// The revision it was edited from.
        base: u64,
        /// The latest revision.
        latest: u64,
    },
    /// The files failed the checks; nothing was stored.
    Refused {
        /// Every defect found.
        defects: Vec<Defect>,
    },
    /// A log record is not one this service wrote.
    MalformedRecord {
        /// What is wrong with it.
        reason: String,
    },
    /// A record does not follow from the state before it.
    ReplayDiverged {
        /// The record's position.
        position: u64,
        /// What the state required.
        expected: String,
    },
    /// The log failed.
    Log(LogError),
}

impl ConfigError {
    /// [`ConfigErrorEnum::UnknownTarget`].
    #[must_use]
    pub fn unknown_target(target: &str) -> Self {
        Self(ConfigErrorEnum::UnknownTarget {
            target: target.to_string(),
        })
    }

    /// [`ConfigErrorEnum::Retired`].
    #[must_use]
    pub fn retired(target: &str) -> Self {
        Self(ConfigErrorEnum::Retired {
            target: target.to_string(),
        })
    }

    /// [`ConfigErrorEnum::NoActiveRevision`].
    #[must_use]
    pub fn no_active_revision(target: &str) -> Self {
        Self(ConfigErrorEnum::NoActiveRevision {
            target: target.to_string(),
        })
    }

    /// [`ConfigErrorEnum::UnknownRevision`].
    #[must_use]
    pub fn unknown_revision(target: &str, revision: u64) -> Self {
        Self(ConfigErrorEnum::UnknownRevision {
            target: target.to_string(),
            revision,
        })
    }

    /// [`ConfigErrorEnum::UnknownInstance`].
    #[must_use]
    pub fn unknown_instance(target: &str, revision: u64, instance: &str) -> Self {
        Self(ConfigErrorEnum::UnknownInstance {
            target: target.to_string(),
            revision,
            instance: instance.to_string(),
        })
    }

    /// [`ConfigErrorEnum::Stale`].
    #[must_use]
    pub fn stale(target: &str, base: u64, latest: u64) -> Self {
        Self(ConfigErrorEnum::Stale {
            target: target.to_string(),
            base,
            latest,
        })
    }

    /// [`ConfigErrorEnum::Refused`].
    #[must_use]
    pub fn refused(defects: Vec<Defect>) -> Self {
        Self(ConfigErrorEnum::Refused { defects })
    }

    /// [`ConfigErrorEnum::MalformedRecord`].
    #[must_use]
    pub fn malformed_record(reason: impl Into<String>) -> Self {
        Self(ConfigErrorEnum::MalformedRecord {
            reason: reason.into(),
        })
    }

    /// [`ConfigErrorEnum::ReplayDiverged`].
    #[must_use]
    pub fn replay_diverged(position: u64, expected: impl Into<String>) -> Self {
        Self(ConfigErrorEnum::ReplayDiverged {
            position,
            expected: expected.into(),
        })
    }
}

impl From<LogError> for ConfigError {
    fn from(error: LogError) -> Self {
        Self(ConfigErrorEnum::Log(error))
    }
}
