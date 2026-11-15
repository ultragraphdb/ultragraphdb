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
    pin_skip: Option<u32>,
}

impl RuntimeTable {
    pub(crate) fn read(reader: &mut SettingsReader) -> Result<Self, ServiceConfigError> {
        Ok(Self {
            pin_cores: reader.boolean("runtime", "pin_cores")?,
            pin_skip: reader.u32("runtime", "pin_skip")?,
        })
    }

    /// `pin_cores`: whether each runtime asks to bind to one CPU.
    #[must_use]
    pub fn pin_cores(&self) -> Option<bool> {
        self.pin_cores
    }

    /// `pin_skip`: how many of the CPUs the role is permitted, from the lowest, its runtimes leave free, for the CPUs a
    /// host gives its interrupts; none when absent.
    #[must_use]
    pub fn pin_skip(&self) -> Option<u32> {
        self.pin_skip
    }
}
