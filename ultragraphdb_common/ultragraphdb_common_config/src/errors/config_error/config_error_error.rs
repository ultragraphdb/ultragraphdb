/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use crate::errors::config_error::{ConfigError, ConfigErrorEnum};

impl Error for ConfigError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.0 {
            ConfigErrorEnum::Log(error) => Some(error),
            ConfigErrorEnum::UnknownTarget { .. }
            | ConfigErrorEnum::Retired { .. }
            | ConfigErrorEnum::NoActiveRevision { .. }
            | ConfigErrorEnum::UnknownRevision { .. }
            | ConfigErrorEnum::UnknownInstance { .. }
            | ConfigErrorEnum::Stale { .. }
            | ConfigErrorEnum::Refused { .. }
            | ConfigErrorEnum::MalformedRecord { .. }
            | ConfigErrorEnum::ReplayDiverged { .. } => None,
        }
    }
}
