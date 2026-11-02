/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::position::Position;

/// A round as the commit step hands it to the log: its position, its parent (the checksum of the round before it, or
/// the last checkpoint's checksum for the first round after a checkpoint), the first commit version and transaction
/// count it records, and its body, the decisions and admitted write-set records, which the log does not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RoundRef<'a> {
    position: Position,
    parent: u64,
    first_version: u64,
    transactions: u32,
    body: &'a [u8],
}

impl<'a> RoundRef<'a> {
    /// The round at `position` naming `parent`, recording `transactions` transactions from commit version
    /// `first_version`, its body `body`.
    pub fn new(
        position: Position,
        parent: u64,
        first_version: u64,
        transactions: u32,
        body: &'a [u8],
    ) -> Self {
        Self {
            position,
            parent,
            first_version,
            transactions,
            body,
        }
    }

    /// The round's place in the log.
    pub fn position(&self) -> Position {
        self.position
    }

    /// The checksum of the round before it, or of the last checkpoint for the first round after one.
    pub fn parent(&self) -> u64 {
        self.parent
    }

    /// The first commit version the round records.
    pub fn first_version(&self) -> u64 {
        self.first_version
    }

    /// The number of transactions the round records.
    pub fn transactions(&self) -> u32 {
        self.transactions
    }

    /// The round's body, which the log does not read.
    pub fn body(&self) -> &'a [u8] {
        self.body
    }
}
