/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::service_config_error::ServiceConfigError;
use crate::types::settings_reader::SettingsReader;

/// The `[compute]` table: a compute process's own place.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComputeTable {
    role_address: Option<String>,
}

impl ComputeTable {
    pub(crate) fn read(reader: &mut SettingsReader) -> Result<Self, ServiceConfigError> {
        Ok(Self {
            role_address: reader.string("compute", "role_address")?,
        })
    }

    /// `role_address`: the address the frontend reaches it at.
    #[must_use]
    pub fn role_address(&self) -> Option<&str> {
        self.role_address.as_deref()
    }
}
