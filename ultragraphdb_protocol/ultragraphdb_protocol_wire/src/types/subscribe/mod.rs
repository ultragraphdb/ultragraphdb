/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod subscribe_body_builder;

/// Storage asks for the sequencer's epoch closes (`PROTOCOL.md` §13.3), answered with EpochClose frames on the
/// request's stream.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `from` | epoch: the last epoch whose close the subscriber holds; 0 for none |
///
/// No sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Subscribe {
    from: u64,
}

impl Subscribe {
    /// A subscription to the closes after epoch `from`.
    #[must_use]
    pub fn new(from: u64) -> Self {
        Self { from }
    }

    /// The last epoch whose close the subscriber holds; 0 for none.
    #[must_use]
    pub fn from(&self) -> u64 {
        self.from
    }
}
