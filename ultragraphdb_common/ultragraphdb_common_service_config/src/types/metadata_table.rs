/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::service_config_error::ServiceConfigError;
use crate::types::settings_reader::SettingsReader;

/// The `[metadata]` table: the metadata group's bounds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetadataTable {
    block_size_max: Option<u64>,
    sessions_max: Option<u32>,
    snapshot_rounds: Option<u32>,
}

impl MetadataTable {
    pub(crate) fn read(reader: &mut SettingsReader) -> Result<Self, ServiceConfigError> {
        Ok(Self {
            block_size_max: reader.u64("metadata", "block_size_max")?,
            sessions_max: reader.u32("metadata", "sessions_max")?,
            snapshot_rounds: reader.u32("metadata", "snapshot_rounds")?,
        })
    }

    /// `block_size_max`: the largest sequence block a partition may lease.
    pub fn block_size_max(&self) -> Option<u64> {
        self.block_size_max
    }

    /// `sessions_max`: the most sessions the registry holds.
    pub fn sessions_max(&self) -> Option<u32> {
        self.sessions_max
    }

    /// `snapshot_rounds`: the rounds between two snapshots.
    pub fn snapshot_rounds(&self) -> Option<u32> {
        self.snapshot_rounds
    }
}
