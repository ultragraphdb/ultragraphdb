/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::position::Position;
use crate::types::round_ref::RoundRef;

/// A round as the log returns it from a read: the fields it was appended with, verified, and its `checksum`, the
/// identity the next round names as its parent.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Round {
    position: Position,
    parent: u64,
    checksum: u64,
    first_version: u64,
    transactions: u32,
    body: Vec<u8>,
}

impl Round {
    pub fn new(
        position: Position,
        parent: u64,
        checksum: u64,
        first_version: u64,
        transactions: u32,
        body: Vec<u8>,
    ) -> Self {
        Self {
            position,
            parent,
            checksum,
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

    /// The round's identity: its frame's header checksum.
    pub fn checksum(&self) -> u64 {
        self.checksum
    }

    pub fn first_version(&self) -> u64 {
        self.first_version
    }

    pub fn transactions(&self) -> u32 {
        self.transactions
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// The round as it was appended.
    pub fn as_round_ref(&self) -> RoundRef<'_> {
        RoundRef::new(
            self.position,
            self.parent,
            self.first_version,
            self.transactions,
            &self.body,
        )
    }
}
