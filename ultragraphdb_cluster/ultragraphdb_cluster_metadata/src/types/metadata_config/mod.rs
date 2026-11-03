/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_cluster::ClusterError;

/// The metadata group's settings: the largest sequence block a partition may lease, `block_size_max`; the most
/// sessions the registry holds, `sessions_max`, past which it evicts the least recently registered; and the rounds
/// between two snapshots, `snapshot_rounds`, after which [`MetadataGroup::snapshot_due`](crate::MetadataGroup) says
/// a snapshot is due and the log is trimmed below it.
///
/// A setting changed between two starts changes only the commands decided after the restart: every command records
/// its decision (a lease its block, a registration the sessions it evicts), so replay never reads the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MetadataConfig {
    block_size_max: u64,
    sessions_max: u32,
    snapshot_rounds: u32,
}

impl MetadataConfig {
    /// `block_size_max` unless set otherwise, 1,048,576 sequences.
    pub const BLOCK_SIZE_MAX: u64 = 1 << 20;

    /// The largest `block_size_max`, 2^32 sequences.
    pub const BLOCK_SIZE_MAX_MAX: u64 = 1 << 32;

    /// `sessions_max` unless set otherwise, 4,096: the partitions' session tables' default bound.
    pub const SESSIONS_MAX: u32 = 4_096;

    /// The largest `sessions_max`, 1,048,576.
    pub const SESSIONS_MAX_MAX: u32 = 1 << 20;

    /// `snapshot_rounds` unless set otherwise, 10,000.
    pub const SNAPSHOT_ROUNDS: u32 = 10_000;

    /// The largest `snapshot_rounds`, 1,048,576.
    pub const SNAPSHOT_ROUNDS_MAX: u32 = 1 << 20;

    /// The settings with every value at its default.
    #[must_use]
    pub fn new() -> Self {
        Self {
            block_size_max: Self::BLOCK_SIZE_MAX,
            sessions_max: Self::SESSIONS_MAX,
            snapshot_rounds: Self::SNAPSHOT_ROUNDS,
        }
    }

    /// The settings leasing blocks of at most `block_size_max` sequences.
    ///
    /// # Errors
    ///
    /// [`ClusterError::setting_below`] when `block_size_max` is 0 and [`ClusterError::setting_above`] when it is above
    /// [`MetadataConfig::BLOCK_SIZE_MAX_MAX`].
    pub fn with_block_size_max(self, block_size_max: u64) -> Result<Self, ClusterError> {
        let block_size_max = bounded("block_size_max", block_size_max, Self::BLOCK_SIZE_MAX_MAX)?;
        Ok(Self {
            block_size_max,
            ..self
        })
    }

    /// The settings holding at most `sessions_max` sessions.
    ///
    /// # Errors
    ///
    /// [`ClusterError::setting_below`] when `sessions_max` is 0 and [`ClusterError::setting_above`] when it is above
    /// [`MetadataConfig::SESSIONS_MAX_MAX`].
    pub fn with_sessions_max(self, sessions_max: u32) -> Result<Self, ClusterError> {
        let sessions_max = bounded(
            "sessions_max",
            u64::from(sessions_max),
            u64::from(Self::SESSIONS_MAX_MAX),
        )?;
        Ok(Self {
            // At most SESSIONS_MAX_MAX, a u32.
            sessions_max: u32::try_from(sessions_max).unwrap_or(Self::SESSIONS_MAX_MAX),
            ..self
        })
    }

    /// The settings snapshotting every `snapshot_rounds` rounds.
    ///
    /// # Errors
    ///
    /// [`ClusterError::setting_below`] when `snapshot_rounds` is 0 and [`ClusterError::setting_above`] when it is
    /// above [`MetadataConfig::SNAPSHOT_ROUNDS_MAX`].
    pub fn with_snapshot_rounds(self, snapshot_rounds: u32) -> Result<Self, ClusterError> {
        let snapshot_rounds = bounded(
            "snapshot_rounds",
            u64::from(snapshot_rounds),
            u64::from(Self::SNAPSHOT_ROUNDS_MAX),
        )?;
        Ok(Self {
            // At most SNAPSHOT_ROUNDS_MAX, a u32.
            snapshot_rounds: u32::try_from(snapshot_rounds).unwrap_or(Self::SNAPSHOT_ROUNDS_MAX),
            ..self
        })
    }

    /// The most sequences one lease takes.
    #[must_use]
    pub fn block_size_max(&self) -> u64 {
        self.block_size_max
    }

    /// The most sessions the registry holds.
    #[must_use]
    pub fn sessions_max(&self) -> u32 {
        self.sessions_max
    }

    /// The rounds between two snapshots.
    #[must_use]
    pub fn snapshot_rounds(&self) -> u32 {
        self.snapshot_rounds
    }
}

/// `value` of the setting `setting`, refused below 1 or above `maximum`.
fn bounded(setting: &'static str, value: u64, maximum: u64) -> Result<u64, ClusterError> {
    if value == 0 {
        return Err(ClusterError::setting_below(setting, 0, "its minimum", 1));
    }
    if value > maximum {
        return Err(ClusterError::setting_above(setting, value, maximum));
    }
    Ok(value)
}

impl Default for MetadataConfig {
    fn default() -> Self {
        Self::new()
    }
}
