/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::service_config_error::ServiceConfigError;
use crate::types::settings_reader::SettingsReader;

/// The `[compute]` table: a compute node's own settings. It holds none: compute listens at no role address (stage 5.9), so
/// `compute.role_address` of a file written before is refused as an unknown setting by its name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComputeTable {}

impl ComputeTable {
    pub(crate) fn read(_reader: &mut SettingsReader) -> Result<Self, ServiceConfigError> {
        Ok(Self {})
    }
}
