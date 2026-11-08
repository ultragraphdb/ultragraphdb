/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::service_config_error::ServiceConfigError;
use crate::types::settings_reader::SettingsReader;

/// The `[observability]` table: the listener of `/health` and `/ready`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObservabilityTable {
    address: Option<String>,
}

impl ObservabilityTable {
    pub(crate) fn read(reader: &mut SettingsReader) -> Result<Self, ServiceConfigError> {
        Ok(Self {
            address: reader.string("observability", "address")?,
        })
    }

    /// `address`: the address it listens at.
    #[must_use]
    pub fn address(&self) -> Option<&str> {
        self.address.as_deref()
    }
}
