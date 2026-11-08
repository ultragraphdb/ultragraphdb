/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU32;

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
    derivation: NonZeroU32,
    body: Vec<u8>,
}

impl Round {
    /// The round at `position` read back with its `checksum`, naming `parent`, recording `transactions` decisions
    /// from commit version `first_version` under derivation version `derivation`, its body `body`.
    pub fn new(
        position: Position,
        parent: u64,
        checksum: u64,
        first_version: u64,
        transactions: u32,
        derivation: NonZeroU32,
        body: Vec<u8>,
    ) -> Self {
        Self {
            position,
            parent,
            checksum,
            first_version,
            transactions,
            derivation,
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

    /// The round's identity: its frame's header checksum.
    pub fn checksum(&self) -> u64 {
        self.checksum
    }

    /// The first commit version the round records.
    pub fn first_version(&self) -> u64 {
        self.first_version
    }

    /// The number of decisions the round records, one per entry.
    pub fn transactions(&self) -> u32 {
        self.transactions
    }

    /// The version of the derivation that turns the round's quads into keys.
    #[must_use]
    pub fn derivation(&self) -> NonZeroU32 {
        self.derivation
    }

    /// The round's body, which the log does not read.
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
            self.derivation,
            &self.body,
        )
    }
}
