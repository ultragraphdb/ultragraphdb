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

/// `SaltedPassword` (RFC 5802 §3): `Hi` of the prepared `password` over `salt` and `iterations`, which RFC 5802 §2.2
/// defines as PBKDF2-HMAC-SHA256's one block: `U1 = HMAC(password, salt ‖ INT(1))`, each `Ui = HMAC(password, Ui-1)`,
/// all XORed together. The password is keyed once and the keyed state cloned per iteration.
pub(crate) fn salted(password: &str, salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut out = [0u8; 32];
    // As in `hmac`: HMAC takes a key of any length, so the zeros are never returned.
    if let Ok(keyed) = <Hmac<Sha256> as KeyInit>::new_from_slice(password.as_bytes()) {
        let mut mac = keyed.clone();
        mac.update(salt);
        mac.update(&1u32.to_be_bytes());
        let mut u: [u8; 32] = mac.finalize().into_bytes().into();
        out = u;
        for _ in 1..iterations {
            let mut round = keyed.clone();
            round.update(&u);
            u = round.finalize().into_bytes().into();
            out.iter_mut().zip(u).for_each(|(o, b)| *o ^= b);
        }
    }
    out
}

/// Whether `a` and `b` are equal, compared in constant time.
pub(crate) fn same(a: &[u8], b: &[u8]) -> bool {
    a.ct_eq(b).into()
}

/// `text` prepared with SASLprep (RFC 4013), refused as [`ScramError::prepare`].
pub(crate) fn prepare(text: &str) -> Result<String, ScramError> {
    stringprep::saslprep(text)
        .map(|prepared| prepared.into_owned())
        .map_err(|_| ScramError::prepare())
}
