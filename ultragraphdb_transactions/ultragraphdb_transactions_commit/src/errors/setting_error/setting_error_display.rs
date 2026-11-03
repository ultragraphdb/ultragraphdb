/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::setting_error::{SettingError, SettingErrorEnum};

impl fmt::Display for SettingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            SettingErrorEnum::CloseRequestRange { value } => {
                write!(f, "close_request_ms is {value}, outside 1 to 60000")
            }
            SettingErrorEnum::CloseRequestBelowEpochs {
                value,
                epoch_ms_max,
            } => write!(
                f,
                "close_request_ms is {value}, below twice epoch_ms_max ({epoch_ms_max})"
            ),
        }
    }
}
