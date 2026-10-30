/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::errors::scram_error::ScramError;

/// HMAC-SHA256 of `data` under `key`.
pub(crate) fn hmac(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    // HMAC takes a key of any length, so the construction does not fail; were it to, the zeros verify nothing.
    if let Ok(mut mac) = <Hmac<Sha256> as KeyInit>::new_from_slice(key) {
        mac.update(data);
        out.copy_from_slice(&mac.finalize().into_bytes());
    }
    out
}

/// SHA-256 of `data`.
pub(crate) fn sha256(data: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&Sha256::digest(data));
    out
}

/// `SaltedPassword` (RFC 5802 §3): PBKDF2-HMAC-SHA256 of the prepared `password` over `salt` and `iterations`.
pub(crate) fn salted(password: &str, salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut out = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, iterations, &mut out);
    out
}

/// Whether `a` and `b` are equal, compared in constant time.
pub(crate) fn same(a: &[u8], b: &[u8]) -> bool {
    a.ct_eq(b).into()
}

/// `text` prepared with SASLprep (RFC 4013), refused as [`ScramError::Prepare`].
pub(crate) fn prepare(text: &str) -> Result<String, ScramError> {
    stringprep::saslprep(text)
        .map(|prepared| prepared.into_owned())
        .map_err(|_| ScramError::Prepare())
}
