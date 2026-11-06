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
    bootstrap_secret_file: Option<String>,
    catalog_check_ms: Option<u64>,
}

impl MetadataTable {
    pub(crate) fn read(reader: &mut SettingsReader) -> Result<Self, ServiceConfigError> {
        Ok(Self {
            block_size_max: reader.u64("metadata", "block_size_max")?,
            sessions_max: reader.u32("metadata", "sessions_max")?,
            snapshot_rounds: reader.u32("metadata", "snapshot_rounds")?,
            bootstrap_secret_file: reader.string("metadata", "bootstrap_secret_file")?,
            catalog_check_ms: reader.u64("metadata", "catalog_check_ms")?,
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

    /// `catalog_check_ms`: how long the frontend waits for each partition's report of a tightening's check.
    pub fn catalog_check_ms(&self) -> Option<u64> {
        self.catalog_check_ms
    }

    /// `bootstrap_secret_file`: the file holding the bootstrap administrator's password, read at the cluster's first start.
    pub fn bootstrap_secret_file(&self) -> Option<&str> {
        self.bootstrap_secret_file.as_deref()
    }
}
