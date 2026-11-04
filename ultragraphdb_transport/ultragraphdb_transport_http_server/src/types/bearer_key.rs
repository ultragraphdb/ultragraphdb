/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::errors::http_server_error::{HttpServerError, HttpServerErrorEnum};

/// The fewest bytes a key holds: 256 bits, when drawn at random as a key must be.
const KEY_BYTES_MIN: usize = 32;

/// What authorises a request: a key's SHA-256 digest, the key itself held nowhere, compared with what a request presents
/// in constant time. So a role can verify a key whose holder could otherwise change every setting, without holding it.
#[derive(Clone, PartialEq, Eq)]
pub struct BearerKey {
    digest: [u8; 32],
}

impl std::fmt::Debug for BearerKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BearerKey")
    }
}

impl BearerKey {
    /// The key `file` holds: its contents without a final line break.
    ///
    /// # Errors
    ///
    /// [`HttpServerErrorEnum::KeyUnreadable`] for a file that cannot be read as text, and
    /// [`HttpServerErrorEnum::KeyTooShort`] for a key under 32 bytes.
    pub fn read(file: &str) -> Result<Self, HttpServerError> {
        let contents = std::fs::read_to_string(file).map_err(|error| {
            HttpServerError(HttpServerErrorEnum::KeyUnreadable {
                file: file.to_string(),
                reason: error.to_string(),
            })
        })?;
        let line = contents.strip_suffix('\n').unwrap_or(&contents);
        let key = line.strip_suffix('\r').unwrap_or(line);
        if key.len() < KEY_BYTES_MIN {
            return Err(HttpServerError(HttpServerErrorEnum::KeyTooShort {
                file: file.to_string(),
                bytes: key.len(),
                required: KEY_BYTES_MIN,
            }));
        }
        Ok(Self {
            digest: digest_of(key),
        })
    }

    /// The key whose digest is the 64 hexadecimal digits `hex`.
    ///
    /// # Errors
    ///
    /// [`HttpServerErrorEnum::DigestMalformed`] for anything else.
    pub fn from_digest_hex(hex: &str) -> Result<Self, HttpServerError> {
        let malformed = || HttpServerError(HttpServerErrorEnum::DigestMalformed);
        if hex.len() != 64 || !hex.is_ascii() {
            return Err(malformed());
        }
        let mut digest = [0u8; 32];
        for (byte, pair) in digest.iter_mut().zip(hex.as_bytes().as_chunks::<2>().0) {
            let text = std::str::from_utf8(pair).map_err(|_| malformed())?;
            *byte = u8::from_str_radix(text, 16).map_err(|_| malformed())?;
        }
        Ok(Self { digest })
    }

    /// Whether `presented` is the key.
    #[must_use]
    pub fn allows(&self, presented: Option<&str>) -> bool {
        presented.is_some_and(|key| bool::from(digest_of(key).ct_eq(&self.digest)))
    }
}

fn digest_of(key: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&Sha256::digest(key.as_bytes()));
    out
}
