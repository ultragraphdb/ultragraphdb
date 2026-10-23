/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::store_error::{StoreError, StoreErrorEnum};
use crate::utils::go_hex_dump::go_hex_dump;

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            StoreErrorEnum::TsTooOld => f.write_str("Transaction is too old"),
            // Badger's exceedsSize: "%s with size %d exceeded %d limit. %s:\n%s" over hex.Dump(key[:1<<10]).
            StoreErrorEnum::KeyTooLarge { size, limit, head } => write!(
                f,
                "Key with size {size} exceeded {limit} limit. Key:\n{}",
                go_hex_dump(head)
            ),
            StoreErrorEnum::ReadFailed { key, reason } => {
                write!(f, "Read of key {} failed: {reason}", hex(key))
            }
            StoreErrorEnum::Corrupted { key, reason } => {
                write!(f, "Corrupted value of key {}: {reason}", hex(key))
            }
            StoreErrorEnum::WriteFailed(reason) => write!(f, "Write to the store failed: {reason}"),
            StoreErrorEnum::Poisoned => {
                f.write_str("The store is poisoned by an earlier failed write and must be reopened")
            }
        }
    }
}

/// A key as Go's `%x` prints a byte slice.
fn hex(key: &[u8]) -> String {
    key.iter().map(|b| format!("{b:02x}")).collect()
}
