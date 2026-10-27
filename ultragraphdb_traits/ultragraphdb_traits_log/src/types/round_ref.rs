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

    pub fn position(&self) -> Position {
        self.position
    }

    pub fn parent(&self) -> u64 {
        self.parent
    }

    pub fn first_version(&self) -> u64 {
        self.first_version
    }

    pub fn transactions(&self) -> u32 {
        self.transactions
    }

    pub fn body(&self) -> &'a [u8] {
        self.body
    }
}
