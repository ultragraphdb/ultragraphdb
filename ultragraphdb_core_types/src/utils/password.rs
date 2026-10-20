/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/password.go. Changed for ultragraphdb: translated to Rust over the bcrypt
 * crate, with golang.org/x/crypto/bcrypt's hash checks and error texts.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::errors::types_error::TypesError;

const PWD_LEN_LIMIT: usize = 6;
const DEFAULT_COST: u32 = 10;
const MIN_HASH_SIZE: usize = 59;

/// Dgraph's `Encrypt`: a bcrypt hash at cost 10 in Go's `$2a$` form, for passwords of at least 6 bytes.
pub fn encrypt(plain: impl AsRef<[u8]>) -> Result<String, TypesError> {
    let plain = plain.as_ref();
    if plain.len() < PWD_LEN_LIMIT {
        return Err(TypesError::password(
            "Password too short, i.e. should have at least 6 chars",
        ));
    }
    if plain.len() > 72 {
        return Err(TypesError::password(
            "bcrypt: password length exceeds 72 bytes",
        ));
    }
    bcrypt::hash_with_result(plain, DEFAULT_COST)
        .map(|h| h.format_for_version(bcrypt::Version::TwoA))
        .map_err(|e| TypesError::password(e.to_string()))
}

/// Dgraph's `VerifyPassword`: whether `plain` matches the bcrypt hash `encrypted`, with x/crypto's errors.
pub fn verify_password(
    plain: impl AsRef<[u8]>,
    encrypted: impl AsRef<[u8]>,
) -> Result<(), TypesError> {
    let (plain, encrypted) = (plain.as_ref(), encrypted.as_ref());
    if plain.len() < PWD_LEN_LIMIT || encrypted.is_empty() {
        return Err(TypesError::password("Invalid password/crypted string"));
    }
    let h = encrypted;
    if h.len() < MIN_HASH_SIZE {
        return Err(TypesError::password(
            "crypto/bcrypt: hashedSecret too short to be a bcrypted password",
        ));
    }
    if h[0] != b'$' {
        return Err(TypesError::password(format!(
            "crypto/bcrypt: bcrypt hashes must start with '$', but hashedSecret started with '{}'",
            h[0] as char
        )));
    }
    if h[1] > b'2' {
        return Err(TypesError::password(format!(
            "crypto/bcrypt: bcrypt algorithm version '{}' requested is newer than current version '2'",
            h[1] as char
        )));
    }
    let Ok(encrypted) = std::str::from_utf8(encrypted) else {
        return Err(TypesError::password(
            "crypto/bcrypt: hashedPassword is not the hash of the given password",
        ));
    };
    match bcrypt::verify(plain, encrypted) {
        Ok(true) => Ok(()),
        Ok(false) => Err(TypesError::password(
            "crypto/bcrypt: hashedPassword is not the hash of the given password",
        )),
        Err(e) => Err(TypesError::password(format!("crypto/bcrypt: {e}"))),
    }
}
