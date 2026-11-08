/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::service_config_error::ServiceConfigError;
use crate::types::settings_reader::SettingsReader;

/// The `[sequencer]` table: the epochs the sequencer closes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SequencerTable {
    epoch_ms_min: Option<u64>,
    epoch_ms_max: Option<u64>,
}

impl SequencerTable {
    pub(crate) fn read(reader: &mut SettingsReader) -> Result<Self, ServiceConfigError> {
        Ok(Self {
            epoch_ms_min: reader.u64("sequencer", "epoch_ms_min")?,
            epoch_ms_max: reader.u64("sequencer", "epoch_ms_max")?,
        })
    }

    /// `epoch_ms_min`: the shortest epoch, in milliseconds.
    #[must_use]
    pub fn epoch_ms_min(&self) -> Option<u64> {
        self.epoch_ms_min
    }

    /// `epoch_ms_max`: the longest epoch, in milliseconds.
    #[must_use]
    pub fn epoch_ms_max(&self) -> Option<u64> {
        self.epoch_ms_max
    }
}
