/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::term_error::{TermError, TermErrorEnum};

impl fmt::Display for TermError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            TermErrorEnum::Truncated { bytes } => {
                write!(
                    f,
                    "Encoded term {} ends before its tag says it does",
                    hex(bytes)
                )
            }
            TermErrorEnum::UnknownTag { tag } => write!(f, "Unknown term tag {tag:#04x}"),
            TermErrorEnum::UnknownDatatype { code } => {
                write!(f, "Unknown datatype code {code:#04x}")
            }
            TermErrorEnum::Malformed { bytes, reason } => {
                write!(f, "Malformed encoded term {}: {reason}", hex(bytes))
            }
            TermErrorEnum::InvalidVector { lexical, reason } => {
                write!(f, "Invalid vector literal {lexical:?}: {reason}")
            }
            TermErrorEnum::NotALiteral { uid } => {
                write!(f, "Encoded term names UID {uid}, not a literal")
            }
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
