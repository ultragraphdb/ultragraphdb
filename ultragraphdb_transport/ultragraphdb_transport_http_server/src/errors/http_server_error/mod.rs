/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

/// Why a key cannot authorise anything. The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpServerErrorEnum {
    /// The key's file cannot be read.
    KeyUnreadable {
        /// The file.
        file: String,
        /// Why not.
        reason: String,
    },
    /// The key is shorter than a random key is.
    KeyTooShort {
        /// The file.
        file: String,
        /// Its key's bytes.
        bytes: usize,
        /// The fewest a key holds.
        required: usize,
    },
    /// A digest that is not 64 hexadecimal digits.
    DigestMalformed,
}

/// Why a key cannot authorise anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpServerError(pub HttpServerErrorEnum);

impl fmt::Display for HttpServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            HttpServerErrorEnum::KeyUnreadable { file, reason } => {
                write!(f, "the key file {file}: {reason}")
            }
            HttpServerErrorEnum::KeyTooShort {
                file,
                bytes,
                required,
            } => write!(
                f,
                "the key file {file} holds {bytes} bytes; a key holds at least {required}"
            ),
            HttpServerErrorEnum::DigestMalformed => {
                f.write_str("a key's digest is 64 hexadecimal digits")
            }
        }
    }
}

impl std::error::Error for HttpServerError {}
