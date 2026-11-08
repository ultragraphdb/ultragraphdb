/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::service_config_error::ServiceConfigError;
use crate::types::settings_reader::SettingsReader;

/// The `[frontend]` table: the frontend's own place.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrontendTable {
    role_address: Option<String>,
    data_dir: Option<String>,
}

impl FrontendTable {
    pub(crate) fn read(reader: &mut SettingsReader) -> Result<Self, ServiceConfigError> {
        Ok(Self {
            role_address: reader.string("frontend", "role_address")?,
            data_dir: reader.string("frontend", "data_dir")?,
        })
    }

    /// `role_address`: the address roles reach it at.
    #[must_use]
    pub fn role_address(&self) -> Option<&str> {
        self.role_address.as_deref()
    }

    /// `data_dir`: the directory of its metadata group's and sequencer's logs.
    #[must_use]
    pub fn data_dir(&self) -> Option<&str> {
        self.data_dir.as_deref()
    }
}
