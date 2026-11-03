/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use ultragraphdb_core_ids::{Sequence, TypePrefix};
use ultragraphdb_traits_blocks::{BlockSource, SequenceBlock};

use crate::errors::quad_error::QuadError;

/// The sequences a partition allocates, per type, from the blocks its [`BlockSource`] leases: each type holds its
/// current block and the block after it, leased as soon as the current one is begun, so the next block is held before
/// the current one runs out. A sequence is allocated only from a block held, in order, and never twice; a block the
/// partition leased and did not finish is abandoned with it, since the source never leases it again. Nothing is
/// persisted: a restarted partition leases new blocks.
#[derive(Debug)]
pub struct Sequences<B> {
    blocks: B,
    held: BTreeMap<u16, Held>,
}

/// What one type holds: the next sequence of its current block with the block's end, and the block after it.
#[derive(Debug, Default, Clone, Copy)]
struct Held {
    current: Option<(u64, u64)>,
    ahead: Option<SequenceBlock>,
}

impl<B: BlockSource> Sequences<B> {
    /// Sequences leased from `blocks`, holding no block yet.
    pub fn new(blocks: B) -> Self {
        Self {
            blocks,
            held: BTreeMap::new(),
        }
    }

    /// The source the blocks are leased from.
    pub fn blocks(&self) -> &B {
        &self.blocks
    }

    /// The next sequence of the type `prefix`, leasing a block when it holds none ahead.
    ///
    /// # Errors
    ///
    /// [`QuadError::no_sequence`] naming the type when its blocks are used up and the source leases none; nothing is
    /// allocated.
    pub fn next(&mut self, prefix: TypePrefix) -> Result<Sequence, QuadError> {
        let mut held = self.held.get(&prefix.get()).copied().unwrap_or_default();
        if held.current.is_none_or(|(next, end)| next >= end) {
            held.current = held
                .ahead
                .take()
                .or_else(|| self.blocks.lease(prefix))
                .map(|block| (block.start().get(), block.end()));
        }
        if held.ahead.is_none() {
            held.ahead = self.blocks.lease(prefix);
        }
        let allocated = match held.current {
            Some((next, end)) if next < end => {
                held.current = Some((next.saturating_add(1), end));
                Sequence::new(next).map_err(|_| QuadError::no_sequence(prefix))
            }
            Some(_) | None => Err(QuadError::no_sequence(prefix)),
        };
        if let (Ok(sequence), Some((_, end))) = (&allocated, held.current) {
            debug_assert!(sequence.get() < end, "a sequence lies inside a held block");
        }
        let _replaced: Option<Held> = self.held.insert(prefix.get(), held);
        allocated
    }
}
