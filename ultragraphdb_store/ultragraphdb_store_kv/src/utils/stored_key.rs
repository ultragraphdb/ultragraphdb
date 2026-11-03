/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The stored key: the user key with every run of `0x00` bytes written as `0x00` and one code byte, ending in a code
//! that marks the key's end, then the bitwise complement of the version in big-endian order. fjall orders raw bytes,
//! and Dgraph's keys are not prefix-free and mostly zero bytes, so this form keeps Badger's order (`y.CompareKeys`: key
//! first, then newest version first), keeps a key's versions contiguous and writes a key's zero runs in two bytes.
//!
//! A run of `k` zero bytes (`1 ≤ k ≤ RUN`) followed by a non-zero byte is `0x00, MORE - k`, the non-zero byte written
//! after it; a run of `j` zero bytes (`0 ≤ j ≤ RUN`) ending the key is `0x00, END + j`; a run longer than `RUN`
//! writes `0x00, LONG` for each `RUN` bytes it starts with. The codes order as the bytes they stand for: a key ending
//! sorts below every key it begins, a longer run below a shorter one followed by a non-zero byte.

/// The longest run one code stands for.
const RUN: usize = 126;
/// The code of a key ending after a run of `j` zero bytes is `END + j`.
const END: u8 = 0x01;
/// The code of `RUN` zero bytes followed by more zero bytes.
const LONG: u8 = END + RUN as u8 + 1;
/// The code of a run of `k` zero bytes followed by a non-zero byte is `MORE - k`.
const MORE: u8 = 0xFF;
/// The version's eight bytes.
const VERSION: usize = 8;

/// The stored key of version `version` of `user_key`.
pub fn stored_key(user_key: &[u8], version: u64) -> Vec<u8> {
    let mut out = encoded(user_key);
    out.extend_from_slice(&(!version).to_be_bytes());
    out
}

/// The first stored key of `user_key`: its encoding and end code, below every stored version of it, which continue
/// with a version, and above every stored version of each key below it.
pub fn key_start(user_key: &[u8]) -> Vec<u8> {
    encoded(user_key)
}

/// The first stored key above every stored version of `user_key` and below every key that follows it: its encoding
/// with the end code one higher, which is the end code or the long-run code of a key continuing with a zero byte.
pub fn key_end(user_key: &[u8]) -> Vec<u8> {
    let mut out = encoded(user_key);
    if let Some(code) = out.last_mut() {
        *code += 1;
    }
    out
}

/// `user_key` with its zero runs written as codes, then its end code.
fn encoded(user_key: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(user_key.len() + 2 + VERSION);
    let mut run = 0usize;
    for &b in user_key {
        if b == 0 {
            run += 1;
            if run > RUN {
                out.extend_from_slice(&[0x00, LONG]);
                run = 1;
            }
            continue;
        }
        if run > 0 {
            out.extend_from_slice(&[0x00, MORE - run as u8]);
            run = 0;
        }
        out.push(b);
    }
    out.extend_from_slice(&[0x00, END + run as u8]);
    out
}

/// The stored range of every version of every key beginning with `prefix`: from the prefix's first stored key to the
/// first stored key of the least key above all of them, unbounded when there is none.
pub fn prefix_range(prefix: &[u8]) -> (Vec<u8>, Option<Vec<u8>>) {
    let above = prefix.iter().rposition(|b| *b != 0xFF).and_then(|at| {
        let (head, tail) = prefix.split_at_checked(at)?;
        // The byte at `at` is below 0xFF, so it rises by one.
        let raised = tail.first()?.checked_add(1)?;
        Some([head, &[raised]].concat())
    });
    (key_start(prefix), above.map(|a| key_start(&a)))
}

/// The encoded user key of a stored key, its end code included and its version left out; `None` when `stored` is too
/// short to end in them.
pub fn encoded_part(stored: &[u8]) -> Option<&[u8]> {
    let split = stored.len().checked_sub(VERSION)?;
    let encoded = stored.get(..split)?;
    let (marker, end) = encoded.get(split.checked_sub(2)?..)?.split_first()?;
    let end = *end.first()?;
    (*marker == 0x00 && (END..LONG).contains(&end)).then_some(encoded)
}

/// The user key and version a stored key encodes, or why it does not encode one.
pub fn decode_stored_key(stored: &[u8]) -> Result<(Vec<u8>, u64), String> {
    let encoded = encoded_part(stored).ok_or_else(|| {
        format!(
            "stored key of {} bytes does not end in a terminated version",
            stored.len()
        )
    })?;
    let mut user_key = Vec::with_capacity(encoded.len());
    let mut bytes = encoded.iter();
    // Ends: each turn takes at least one byte of the encoded key.
    while let Some(&b) = bytes.next() {
        if b != 0 {
            user_key.push(b);
            continue;
        }
        let run = match bytes.next() {
            Some(&c) if (END..LONG).contains(&c) => {
                user_key.resize(user_key.len() + usize::from(c - END), 0);
                if bytes.next().is_some() {
                    return Err("stored key continues past its end code".to_string());
                }
                break;
            }
            Some(&LONG) => RUN,
            Some(&c) if c > LONG && c < MORE => usize::from(MORE - c),
            _ => return Err("stored key holds a zero byte without a run code".to_string()),
        };
        user_key.resize(user_key.len() + run, 0);
    }
    // `encoded_part` found the version's bytes at the end.
    let version = stored
        .last_chunk::<VERSION>()
        .ok_or_else(|| "stored key ends without its version".to_string())?;
    Ok((user_key, !u64::from_be_bytes(*version)))
}
