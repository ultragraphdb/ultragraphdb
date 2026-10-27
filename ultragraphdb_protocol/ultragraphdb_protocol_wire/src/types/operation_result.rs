/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The result of a commit-time operation in a Commit reply: the UID of the node an insert-if-absent or
/// upsert-and-link found or created, and whether it created it; zero and `false` for compare-and-set and counter add.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OperationResult {
    uid: u64,
    created: bool,
}

impl OperationResult {
    pub fn new(uid: u64, created: bool) -> Self {
        Self { uid, created }
    }

    pub fn uid(&self) -> u64 {
        self.uid
    }

    pub fn created(&self) -> bool {
        self.created
    }
}
