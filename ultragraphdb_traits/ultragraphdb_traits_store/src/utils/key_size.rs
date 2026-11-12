/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreError;

/// The longest user key a write may name: Badger's `maxKeySize` (`txn.go:352`).
pub const MAX_USER_KEY_SIZE: usize = 65_000;

/// The longest stored key, encoded, terminated and versioned: fjall's journal writes a key's length as a `u16`.
pub const MAX_STORED_KEY_SIZE: usize = 65_535;

/// The bytes Badger's error dumps of an oversized key.
const DUMPED_BYTES: usize = 1 << 10;

/// The longest run of zero bytes one code of the stored key stands for.
const RUN: u8 = 126;

/// The length of `user_key` as the store keeps it: every non-zero byte as itself, every run of zero bytes as two bytes
/// for each 126 it holds or begins, the two bytes ending the key, then the eight bytes of the version.
pub fn stored_key_size(user_key: &[u8]) -> usize {
    let (mut size, mut run) = (0usize, 0usize);
    for &b in user_key {
        if b == 0 {
            run += 1;
            continue;
        }
        size += 1 + codes(run);
        run = 0;
    }
    size + codes(run).max(2) + 8
}

/// The bytes of the codes a run of `run` zero bytes takes.
fn codes(run: usize) -> usize {
    2 * run.div_ceil(usize::from(RUN))
}

/// The first `limit` bytes of the stored form of `user_key`, as the store's encoding writes them.
fn stored_head(user_key: &[u8], limit: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(limit + 2);
    // Reset past `RUN`, so it and the codes made of it fit a byte.
    let mut run = 0u8;
    for &b in user_key {
        if out.len() >= limit {
            break;
        }
        if b == 0 {
            run += 1;
            if run > RUN {
                out.extend_from_slice(&[0x00, RUN + 2]);
                run = 1;
            }
            continue;
        }
        if run > 0 {
            out.extend_from_slice(&[0x00, 0xFF - run]);
            run = 0;
        }
        out.push(b);
    }
    out.truncate(limit);
    out
}

/// Refuses `user_key` when it exceeds [`MAX_USER_KEY_SIZE`], with Badger's error over the user key, or when its
/// stored form exceeds [`MAX_STORED_KEY_SIZE`], with the same error over the stored key.
pub fn check_key_size(user_key: &[u8]) -> Result<(), StoreError> {
    if user_key.len() > MAX_USER_KEY_SIZE {
        // The key is longer than the bytes dumped, so `get` always finds them.
        let head = user_key.get(..DUMPED_BYTES).unwrap_or(user_key).to_vec();
        return Err(StoreError::key_too_large(
            user_key.len(),
            MAX_USER_KEY_SIZE,
            head,
        ));
    }
    let stored = stored_key_size(user_key);
    if stored > MAX_STORED_KEY_SIZE {
        // The stored key is longer than 1,024 bytes, so its head is encoded user-key bytes.
        let head = stored_head(user_key, DUMPED_BYTES);
        return Err(StoreError::key_too_large(stored, MAX_STORED_KEY_SIZE, head));
    }
    Ok(())
}
