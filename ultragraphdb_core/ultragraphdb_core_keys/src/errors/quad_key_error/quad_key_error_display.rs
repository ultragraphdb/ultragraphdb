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
            QuadKeyErrorEnum::ZeroUid { key, field } => {
                write!(f, "Key {} has a {field} of 0", hex(key))
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
        }
    }
}

/// The bytes in hexadecimal, as every error names a key.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
