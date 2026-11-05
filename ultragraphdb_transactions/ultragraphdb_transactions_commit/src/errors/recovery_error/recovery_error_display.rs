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
            RecoveryErrorEnum::AppliedPosition { cause } => {
                write!(
                    f,
                    "the store's applied log position could not be read: {cause}"
                )
            }
            RecoveryErrorEnum::StoreNotInLog { position } => write!(
                f,
                "the store's applied log position {position} lies beyond the log's tail: the store is ahead of the log \
                 or was written outside it"
            ),
            RecoveryErrorEnum::Gap { position } => write!(
                f,
                "the log has no round at position {position}, between the store's applied position and the log's tail"
            ),
            RecoveryErrorEnum::BehindCheckpoint {
                applied,
                checkpoint,
            } => write!(
                f,
                "the store's applied log position {applied} is below the log's checkpoint at position {checkpoint}: the \
                 rounds between them are no longer in the log"
            ),
            RecoveryErrorEnum::Derivation {
                position,
                derivation,
            } => write!(
                f,
                "the round at position {position} names derivation {derivation}, which this store does not know: \
                 nothing of it is applied"
            ),
            RecoveryErrorEnum::Catalog {
                position,
                tenant,
                version,
                reason,
            } => write!(
                f,
                "the round at position {position} starts catalog version {version} of tenant {tenant}, which \
                 could not be read: {reason}"
            ),
        }
    }
}
