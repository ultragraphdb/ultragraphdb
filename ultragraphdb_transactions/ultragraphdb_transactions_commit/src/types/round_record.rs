/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a round records besides its body: its first commit version and its transaction count, every decision
/// counted, and how many of them committed. Its committed transaction k has version `first_version + 2k`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RoundRecord {
    first_version: u64,
    transactions: u32,
    committed: u32,
}

impl RoundRecord {
    pub fn new(first_version: u64, transactions: u32, committed: u32) -> Self {
        Self {
            first_version,
            transactions,
            committed,
        }
    }

    pub fn first_version(&self) -> u64 {
        self.first_version
    }

    /// The number of decisions, aborts included: the Round header's transaction count.
    pub fn transactions(&self) -> u32 {
        self.transactions
    }

    pub fn committed(&self) -> u32 {
        self.committed
    }

    /// The version of its last committed transaction, `None` when none committed.
    pub fn last_version(&self) -> Option<u64> {
        let k = u64::from(self.committed.checked_sub(1)?);
        self.first_version.checked_add(k.checked_mul(2)?)
    }
}
