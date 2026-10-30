/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The result of a commit-time operation in a Commit reply: the UID of the node an insert-if-absent or
/// upsert-and-link found or created, and whether it created it; a counter add's new value, an i64 in two's complement
/// in the `uid` field; zero and `false` for compare-and-set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OperationResult {
    uid: u64,
    created: bool,
}

impl OperationResult {
    pub fn new(uid: u64, created: bool) -> Self {
        Self { uid, created }
    }

    /// The result of a counter add whose counter now holds `value`.
    pub fn counter(value: i64) -> Self {
        // Two's complement in the u64 field, as the layout states.
        Self {
            uid: value as u64,
            created: false,
        }
    }

    pub fn uid(&self) -> u64 {
        self.uid
    }

    /// A counter add's new value: the `uid` field read as an i64.
    pub fn counter_value(&self) -> i64 {
        self.uid as i64
    }

    pub fn created(&self) -> bool {
        self.created
    }
}
