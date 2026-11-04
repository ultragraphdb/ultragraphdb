/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::service_config_error::{ServiceConfigError, ServiceConfigErrorEnum};

impl fmt::Display for ServiceConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            ServiceConfigErrorEnum::Unreadable { file, reason } => {
                write!(f, "the settings file {file} cannot be read: {reason}")
            }
            ServiceConfigErrorEnum::Toml(error) => write!(f, "{error}"),
            ServiceConfigErrorEnum::UnknownTable { table } => {
                write!(f, "the table {table} is an unknown setting")
            }
            ServiceConfigErrorEnum::UnknownVariable { variable } => {
                write!(f, "the variable {variable} names no setting")
            }
            ServiceConfigErrorEnum::WrongKindVariable {
                variable,
                value,
                expected,
            } => write!(
                f,
                "the variable {variable} holds {value}; {expected} is expected"
            ),
            ServiceConfigErrorEnum::OutOfRange {
                setting,
                value,
                max,
            } => write!(f, "the setting {setting} holds {value}, outside 0 to {max}"),
            ServiceConfigErrorEnum::Missing { setting } => {
                write!(f, "the setting {setting} is required")
            }
            ServiceConfigErrorEnum::NoEndpointSecurity => write!(
                f,
                "the endpoint must name tls_cert_file and tls_key_file, or set plaintext = true"
            ),
            ServiceConfigErrorEnum::ConflictingEndpointSecurity => write!(
                f,
                "the endpoint names a certificate or key together with plaintext = true"
            ),
            ServiceConfigErrorEnum::IncompleteEndpointSecurity { absent } => {
                write!(
                    f,
                    "the endpoint names a certificate or key without {absent}"
                )
            }
        }
    }
}
