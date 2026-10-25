/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, types/password.go. Changed for ultragraphdb: translated to Rust over the bcrypt
 * crate, with golang.org/x/crypto/bcrypt's hash checks and error texts.
 * Contains code ported from golang.org/x/crypto v0.56.0, bcrypt/bcrypt.go (newFromHash, CompareHashAndPassword),
 * and from Go 1.27, src/encoding/base64/base64.go (Decode). Copyright 2011 The Go Authors, BSD-3-Clause.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::errors::types_error::TypesError;
use ultragraphdb_common_utils::go_strconv;

const PWD_LEN_LIMIT: usize = 6;
const DEFAULT_COST: u32 = 10;
const MIN_HASH_SIZE: usize = 59;
const MIN_COST: i64 = 4;
const MAX_COST: i64 = 31;
const ENCODED_SALT_SIZE: usize = 22;
const ENCODED_HASH_SIZE: usize = 31;

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
///
/// The hash is read as golang.org/x/crypto/bcrypt v0.56.0's `CompareHashAndPassword` reads it: `$`, a major version
/// of at most `2` with an optional minor one, a two-byte cost read by `strconv.Atoi` and checked against 4..=31, a
/// salt of 22 bytes decoded by Go's base64 decoder, and the hash bytes after it, of which the first 31 count.
///
/// Deviation: a salt that decodes to other than 16 bytes, which only the line breaks Go's decoder skips can produce,
/// is reported as a mismatch. Go hashes with the shorter salt and accepts a hash computed with it; the bcrypt crate
/// takes a 16-byte salt only.
pub fn verify_password(
    plain: impl AsRef<[u8]>,
    encrypted: impl AsRef<[u8]>,
) -> Result<(), TypesError> {
    let (plain, h) = (plain.as_ref(), encrypted.as_ref());
    if plain.len() < PWD_LEN_LIMIT || h.is_empty() {
        return Err(TypesError::password("Invalid password/crypted string"));
    }
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
    // decodeVersion skips the byte after the version, and decodeCost the byte after the cost, unread.
    let n = if h[2] == b'$' { 3 } else { 4 };
    let cost = go_strconv::atoi(&h[n..n + 2]).map_err(|e| TypesError::password(e.to_string()))?;
    if !(MIN_COST..=MAX_COST).contains(&cost) {
        return Err(TypesError::password(format!(
            "crypto/bcrypt: cost {cost} is outside allowed inclusive range {MIN_COST}..{MAX_COST}"
        )));
    }
    let (salt, stored) = h[n + 3..].split_at(ENCODED_SALT_SIZE);
    let mut padded = salt.to_vec();
    padded.extend_from_slice(b"==");
    let salt = go_base64_decode(&padded)
        .map_err(|i| TypesError::password(format!("illegal base64 data at input byte {i}")))?;
    let mismatch = || {
        TypesError::password("crypto/bcrypt: hashedPassword is not the hash of the given password")
    };
    let Ok(salt) = <[u8; 16]>::try_from(salt) else {
        return Err(mismatch());
    };
    // The range check above keeps the cost within u32 and within the crate's range.
    let computed = bcrypt::hash_with_salt(plain, cost as u32, salt)
        .map_err(|e| TypesError::password(format!("crypto/bcrypt: {e}")))?
        .format_for_version(bcrypt::Version::TwoA);
    let computed = &computed.as_bytes()[computed.len() - ENCODED_HASH_SIZE..];
    // Go copies the stored hash into a 60-byte buffer, so bytes past the 31st are dropped and missing ones are zero.
    let stored = &stored[..stored.len().min(ENCODED_HASH_SIZE)];
    let differs = stored.len() != computed.len()
        || stored
            .iter()
            .zip(computed)
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            != 0;
    if differs { Err(mismatch()) } else { Ok(()) }
}

/// Go's `base64.Encoding.Decode` over bcrypt's alphabet with `=` padding, not strict, as x/crypto's `base64Decode`
/// uses it: the decoded bytes, or the input index that Go's `CorruptInputError` names.
fn go_base64_decode(src: &[u8]) -> Result<Vec<u8>, usize> {
    const ALPHABET: &[u8; 64] = b"./ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let is_newline = |c: u8| c == b'\n' || c == b'\r';
    let mut out = Vec::with_capacity(src.len() / 4 * 3);
    let mut si = 0;
    while si < src.len() {
        let (mut dbuf, mut dlen, mut j) = ([0u8; 4], 4, 0);
        let mut trailing = None;
        while j < 4 {
            if si == src.len() {
                if j == 0 {
                    return Ok(out);
                }
                return Err(si - j);
            }
            let c = src[si];
            si += 1;
            if let Some(v) = ALPHABET.iter().position(|&a| a == c) {
                dbuf[j] = v as u8;
                j += 1;
                continue;
            }
            if is_newline(c) {
                continue;
            }
            if c != b'=' || j < 2 {
                return Err(si - 1);
            }
            if j == 2 {
                while si < src.len() && is_newline(src[si]) {
                    si += 1;
                }
                if si == src.len() {
                    return Err(src.len());
                }
                if src[si] != b'=' {
                    return Err(si - 1);
                }
                si += 1;
            }
            while si < src.len() && is_newline(src[si]) {
                si += 1;
            }
            if si < src.len() {
                trailing = Some(si);
            }
            dlen = j;
            break;
        }
        let val = dbuf.iter().fold(0u32, |acc, &d| (acc << 6) | u32::from(d));
        out.extend_from_slice(&val.to_be_bytes()[1..dlen]);
        if let Some(i) = trailing {
            return Err(i);
        }
    }
    Ok(out)
}
