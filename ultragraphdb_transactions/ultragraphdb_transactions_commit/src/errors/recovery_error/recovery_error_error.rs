/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::errors::recovery_error::{RecoveryError, RecoveryErrorEnum};

impl std::error::Error for RecoveryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.0 {
            RecoveryErrorEnum::Log { cause } => Some(cause),
            RecoveryErrorEnum::Round { cause, .. } => Some(cause),
            RecoveryErrorEnum::Apply { cause, .. } => Some(cause),
            RecoveryErrorEnum::Sessions { cause } => Some(cause),
            RecoveryErrorEnum::StoreNotInLog { .. } | RecoveryErrorEnum::Gap { .. } => None,
        }
    }
}
