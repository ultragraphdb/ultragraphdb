/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreError;

/// The longest user key a write may name: Badger's `maxKeySize` (`txn.go:352`).
pub const MAX_USER_KEY_SIZE: usize = 65_000;

/// The longest stored key, escaped, terminated and versioned: fjall's journal writes a key's length as a `u16`.
pub const MAX_STORED_KEY_SIZE: usize = 65_535;

/// The bytes Badger's error dumps of an oversized key.
const DUMPED_BYTES: usize = 1 << 10;

/// The length of `user_key` as the store keeps it: every `0x00` byte written as `0x00 0xFF`, then the terminator
/// `0x00 0x01`, then the eight bytes of the version.
pub fn stored_key_size(user_key: &[u8]) -> usize {
    user_key.len() + user_key.iter().filter(|&&b| b == 0).count() + 2 + 8
}

/// Refuses `user_key` when it exceeds [`MAX_USER_KEY_SIZE`], with Badger's error over the user key, or when its
/// stored form exceeds [`MAX_STORED_KEY_SIZE`], with the same error over the stored key.
pub fn check_key_size(user_key: &[u8]) -> Result<(), StoreError> {
    if user_key.len() > MAX_USER_KEY_SIZE {
        let head = user_key[..DUMPED_BYTES].to_vec();
        return Err(StoreError::KeyTooLarge(
            user_key.len(),
            MAX_USER_KEY_SIZE,
            head,
        ));
    }
    let stored = stored_key_size(user_key);
    if stored > MAX_STORED_KEY_SIZE {
        // The stored key is longer than the user key, so its first 1,024 bytes are escaped user-key bytes.
        let head = user_key
            .iter()
            .flat_map(|&b| if b == 0 { vec![0x00, 0xFF] } else { vec![b] })
            .take(DUMPED_BYTES)
            .collect();
        return Err(StoreError::KeyTooLarge(stored, MAX_STORED_KEY_SIZE, head));
    }
    Ok(())
}
