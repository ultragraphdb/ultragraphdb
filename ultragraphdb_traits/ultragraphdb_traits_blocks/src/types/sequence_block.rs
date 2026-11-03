/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::Sequence;

/// A half-open range of one type's sequences, `start..end`, leased to one partition: at least one sequence, the
/// first never 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SequenceBlock {
    start: Sequence,
    end: u64,
}

impl SequenceBlock {
    /// The block `start..end`, or `None` when `start` is 0 or not below `end`.
    pub fn new(start: u64, end: u64) -> Option<Self> {
        let block = Self {
            start: Sequence::new(start).ok()?,
            end,
        };
        (start < end).then_some(block)
    }

    /// The block's first sequence.
    pub fn start(self) -> Sequence {
        self.start
    }

    /// The sequence after the block's last.
    pub fn end(self) -> u64 {
        self.end
    }

    /// How many sequences the block holds, at least 1.
    pub fn count(self) -> u64 {
        debug_assert!(self.start.get() < self.end, "a block holds a sequence");
        // No underflow: a block's start lies below its end.
        self.end - self.start.get()
    }

    /// Whether `sequence` lies in the block.
    pub fn contains(self, sequence: Sequence) -> bool {
        self.start <= sequence && sequence.get() < self.end
    }
}
