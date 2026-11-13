/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * After Dgraph v25.5.0, types/password.go. Changed for ultragraphdb: written in Rust over the bcrypt crate, which
 * hashes, reads a stored hash and compares in constant time.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::errors::types_error::TypesError;

/// The fewest bytes a password holds.
const PWD_LEN_LIMIT: usize = 6;
/// The most bytes bcrypt hashes; a longer password is refused, never cut.
const PWD_LEN_MAX: usize = 72;
/// The work factor of every hash written.
const DEFAULT_COST: u32 = 10;

/// Dgraph's `Encrypt`: a bcrypt hash at cost 10 in the `$2a$` form, for a password of 6 to 72 bytes.
///
/// # Errors
///
/// [`TypesError::password`] for a password of fewer than 6 bytes or more than 72, and the bcrypt crate's failure
/// to draw a salt.
pub fn encrypt(plain: impl AsRef<[u8]>) -> Result<String, TypesError> {
    let plain = plain.as_ref();
    if plain.len() < PWD_LEN_LIMIT {
        return Err(TypesError::password(
            "Password too short, i.e. should have at least 6 chars",
        ));
    }
    if plain.len() > PWD_LEN_MAX {
        return Err(TypesError::password(
            "bcrypt: password length exceeds 72 bytes",
        ));
    }
    bcrypt::hash_with_result(plain, DEFAULT_COST)
        .map(|h| h.format_for_version(bcrypt::Version::TwoA))
        .map_err(|e| TypesError::password(format!("bcrypt: {e}")))
}

/// Dgraph's `VerifyPassword`: whether `plain` is the password the bcrypt hash `encrypted` was made of. The hash is
/// read by the bcrypt crate and the comparison takes the same time whatever differs.
///
/// # Errors
///
/// [`TypesError::password`] for a password of fewer than 6 bytes or an empty hash, a hash that is no bcrypt
/// hash, a password of more than 72 bytes, which no hash was made of, and a password that does not match.
pub fn verify_password(
    plain: impl AsRef<[u8]>,
    encrypted: impl AsRef<[u8]>,
) -> Result<(), TypesError> {
    let (plain, hash) = (plain.as_ref(), encrypted.as_ref());
    if plain.len() < PWD_LEN_LIMIT || hash.is_empty() {
        return Err(TypesError::password("Invalid password/crypted string"));
    }
    let hash = std::str::from_utf8(hash)
        .map_err(|_| TypesError::password("bcrypt: a hash that is no text"))?;
    if plain.len() > PWD_LEN_MAX {
        return Err(TypesError::password(
            "bcrypt: password length exceeds 72 bytes",
        ));
    }
    match bcrypt::verify(plain, hash) {
        Ok(true) => Ok(()),
        Ok(false) => Err(TypesError::password(
            "bcrypt: the password does not match its hash",
        )),
        Err(e) => Err(TypesError::password(format!("bcrypt: {e}"))),
    }
}
