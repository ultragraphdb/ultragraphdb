/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::service_config_error::ServiceConfigError;
use crate::types::settings_reader::SettingsReader;

/// The `[system]` table: the system group a storage instance hosts, the metadata state machine and the sequencer, where
/// it listens and where its logs lie. A storage instance whose settings hold the table hosts the group, and the table
/// holds both its settings, so a value of this type names both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemTable {
    role_address: String,
    data_dir: String,
}

impl SystemTable {
    /// The table the settings hold, or `None` when they hold no `[system]` table. A table lacking either setting is
    /// refused naming it, as [`ServiceConfigError::missing`] does a setting a role requires; a collecting reader records
    /// each absent one, and reads no table.
    pub(crate) fn read(reader: &mut SettingsReader) -> Result<Option<Self>, ServiceConfigError> {
        if !reader.holds("system") {
            return Ok(None);
        }
        let role_address = reader.required_string("system", "role_address")?;
        let data_dir = reader.required_string("system", "data_dir")?;
        Ok(role_address
            .zip(data_dir)
            .map(|(role_address, data_dir)| Self {
                role_address,
                data_dir,
            }))
    }

    /// `role_address`: where the system group listens for role connections, an address of its own beside the
    /// partitions'.
    #[must_use]
    pub fn role_address(&self) -> &str {
        &self.role_address
    }

    /// `data_dir`: the directory holding the system group's two logs and their snapshots, `meta/` and `seq/`.
    #[must_use]
    pub fn data_dir(&self) -> &str {
        &self.data_dir
    }
}
