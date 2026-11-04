/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::service_config_error::ServiceConfigError;
use crate::types::settings_reader::SettingsReader;

/// The `[runtime]` table: how a role's runtimes meet the machine.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeTable {
    pin_cores: Option<bool>,
}

impl RuntimeTable {
    pub(crate) fn read(reader: &mut SettingsReader) -> Result<Self, ServiceConfigError> {
        Ok(Self {
            pin_cores: reader.boolean("runtime", "pin_cores")?,
        })
    }

    /// `pin_cores`: whether each runtime asks to bind to one CPU.
    pub fn pin_cores(&self) -> Option<bool> {
        self.pin_cores
    }
}
