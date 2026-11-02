/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::quad_key_error::{QuadKeyError, QuadKeyErrorEnum};

impl fmt::Display for QuadKeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            QuadKeyErrorEnum::Truncated { key } => {
                write!(f, "Key {} ends before its fields do", hex(key))
            }
            QuadKeyErrorEnum::TrailingBytes { key } => {
                write!(f, "Key {} has bytes after its fields", hex(key))
            }
            QuadKeyErrorEnum::UnknownKeyspace { key } => {
                write!(f, "Key {} belongs to no keyspace", hex(key))
            }
            QuadKeyErrorEnum::InvalidId { key, field, error } => {
                write!(f, "Key {} has an invalid {field}: {error}", hex(key))
            }
            QuadKeyErrorEnum::UnknownDirection { key } => {
                write!(f, "Count key {} has an unknown direction", hex(key))
            }
            QuadKeyErrorEnum::BadObject { key, error } => {
                write!(f, "Key {} has an unreadable object: {error}", hex(key))
            }
            QuadKeyErrorEnum::VectorObject { key } => {
                write!(f, "Key {} names a vector as its object", hex(key))
            }
            QuadKeyErrorEnum::NotAList { key } => {
                write!(f, "Key {} is not a list's and does not split", hex(key))
            }
            QuadKeyErrorEnum::GroupRange { key, group } => {
                write!(
                    f,
                    "Mark key {} has bucket group {group}, past 18 bits",
                    hex(key)
                )
            }
            QuadKeyErrorEnum::FingerprintStart { key } => write!(
                f,
                "Value-index key {} splits at an ID alone, not a fingerprint",
                hex(key)
            ),
        }
    }
}

/// The bytes in hexadecimal, as every error names a key.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
