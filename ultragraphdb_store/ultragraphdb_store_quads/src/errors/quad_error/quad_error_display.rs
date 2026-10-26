/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::quad_error::{QuadError, QuadErrorEnum};

impl fmt::Display for QuadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            QuadErrorEnum::Store(e) => write!(f, "{e}"),
            QuadErrorEnum::Posting(e) => write!(f, "{e}"),
            QuadErrorEnum::Term(e) => write!(f, "{e}"),
            QuadErrorEnum::Key(e) => write!(f, "{e}"),
            QuadErrorEnum::Mapping(e) => write!(f, "{e}"),
            QuadErrorEnum::Catalog(e) => write!(f, "{e}"),
            QuadErrorEnum::Corrupted { key, reason } => {
                let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
                write!(f, "Corrupted dictionary at key {hex}: {reason}")
            }
            QuadErrorEnum::Aborted => f.write_str("Transaction has been aborted. Please retry"),
            QuadErrorEnum::UidsExhausted { namespace } => {
                write!(
                    f,
                    "Namespace {namespace} has no UID left below the entity-type bits"
                )
            }
        }
    }
}
