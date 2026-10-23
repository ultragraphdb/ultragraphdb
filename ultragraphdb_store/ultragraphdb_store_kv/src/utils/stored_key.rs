/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The stored key: the user key with every `0x00` byte written as `0x00 0xFF`, then the terminator `0x00 0x01`, then
//! the bitwise complement of the version in big-endian order. fjall orders raw bytes, and Dgraph's keys are not
//! prefix-free, so this form is what keeps Badger's order (`y.CompareKeys`: key first, then newest version first),
//! keeps a key's versions contiguous and maps a user-key prefix to a stored-key prefix.

/// The byte that follows an escaped `0x00` of the user key.
const ESCAPE: u8 = 0xFF;
/// The byte that follows the `0x00` ending the user key.
const TERMINATOR: u8 = 0x01;
/// The terminator and the version's eight bytes.
const SUFFIX: usize = 2 + 8;

/// `user_key` with every `0x00` byte written as `0x00 0xFF`: the stored prefix of every key beginning with it.
pub fn escape(user_key: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(user_key.len() + SUFFIX);
    push_escaped(&mut out, user_key);
    out
}

fn push_escaped(out: &mut Vec<u8>, user_key: &[u8]) {
    for &b in user_key {
        out.push(b);
        if b == 0 {
            out.push(ESCAPE);
        }
    }
}

/// The stored key of version `version` of `user_key`.
pub fn stored_key(user_key: &[u8], version: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(user_key.len() + SUFFIX + 4);
    push_escaped(&mut out, user_key);
    out.extend_from_slice(&[0x00, TERMINATOR]);
    out.extend_from_slice(&(!version).to_be_bytes());
    out
}

/// The first stored key above every stored version of `user_key` and below every key that follows it: the escaped
/// key, then `0x00 0x02`. A longer key continuing with `0x00` continues with `0x00 0xFF`, and one continuing with any
/// other byte is above the escaped key followed by `0x00`.
pub fn key_end(user_key: &[u8]) -> Vec<u8> {
    let mut out = escape(user_key);
    out.extend_from_slice(&[0x00, TERMINATOR + 1]);
    out
}

/// The escaped user key of a stored key, without its terminator and version; `None` when `stored` is too short to
/// end in them.
pub fn escaped_part(stored: &[u8]) -> Option<&[u8]> {
    let split = stored.len().checked_sub(SUFFIX)?;
    let (escaped, suffix) = stored.split_at(split);
    (suffix[..2] == [0x00, TERMINATOR]).then_some(escaped)
}

/// The user key and version a stored key encodes, or why it does not encode one.
pub fn decode_stored_key(stored: &[u8]) -> Result<(Vec<u8>, u64), String> {
    let escaped = escaped_part(stored).ok_or_else(|| {
        format!(
            "stored key of {} bytes does not end in a terminated version",
            stored.len()
        )
    })?;
    let mut user_key = Vec::with_capacity(escaped.len());
    let mut bytes = escaped.iter();
    while let Some(&b) = bytes.next() {
        user_key.push(b);
        if b == 0 && bytes.next() != Some(&ESCAPE) {
            return Err("stored key holds an unescaped zero byte".to_string());
        }
    }
    let mut version = [0u8; 8];
    version.copy_from_slice(&stored[stored.len() - 8..]);
    Ok((user_key, !u64::from_be_bytes(version)))
}
