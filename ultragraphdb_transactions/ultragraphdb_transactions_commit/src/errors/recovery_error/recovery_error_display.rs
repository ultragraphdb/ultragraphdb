/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::recovery_error::{RecoveryError, RecoveryErrorEnum};

impl fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            RecoveryErrorEnum::Log { cause } => write!(f, "the log could not be read: {cause}"),
            RecoveryErrorEnum::Round { position, cause } => {
                write!(f, "the round at position {position} is malformed: {cause}")
            }
            RecoveryErrorEnum::Apply { position, cause } => {
                write!(
                    f,
                    "the store refused the round at position {position}: {cause}"
                )
            }
            RecoveryErrorEnum::Sessions { cause } => {
                write!(f, "the store's session table could not be read: {cause}")
            }
            RecoveryErrorEnum::StoreNotInLog { version } => write!(
                f,
                "the store's applied version {version} is no round's last version in the log: the store is ahead \
                 of the log or was written outside it"
            ),
            RecoveryErrorEnum::Gap { checkpoint } => write!(
                f,
                "the store is empty and the log was trimmed through position {checkpoint}: the rounds it covered \
                 are lost"
            ),
        }
    }
}
