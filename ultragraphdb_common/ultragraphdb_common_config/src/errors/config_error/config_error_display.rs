/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::config_error::{ConfigError, ConfigErrorEnum};

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            ConfigErrorEnum::UnknownTarget { target } => {
                write!(f, "target {target} does not exist")
            }
            ConfigErrorEnum::Retired { target } => write!(
                f,
                "target {target} is retired; activating a revision ends the retirement"
            ),
            ConfigErrorEnum::NoActiveRevision { target } => {
                write!(f, "target {target} has no active revision")
            }
            ConfigErrorEnum::UnknownRevision { target, revision } => {
                write!(f, "target {target} holds no revision {revision}")
            }
            ConfigErrorEnum::UnknownInstance {
                target,
                revision,
                instance,
            } => write!(
                f,
                "target {target}, revision {revision}, holds no instance {instance}"
            ),
            ConfigErrorEnum::Stale {
                target,
                base,
                latest,
            } => write!(
                f,
                "target {target}'s latest revision is {latest}, not {base}; edit from revision {latest}"
            ),
            ConfigErrorEnum::Refused { defects } => {
                for (at, defect) in defects.iter().enumerate() {
                    if at > 0 {
                        f.write_str("\n")?;
                    }
                    write!(f, "{defect}")?;
                }
                Ok(())
            }
            ConfigErrorEnum::MalformedRecord { reason } => {
                write!(f, "a stored record is malformed: {reason}")
            }
            ConfigErrorEnum::ReplayDiverged { position, expected } => {
                write!(f, "the record at position {position} is not {expected}")
            }
            ConfigErrorEnum::Log(error) => write!(f, "the log: {error}"),
        }
    }
}
