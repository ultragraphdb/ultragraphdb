/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, VecDeque};
use std::num::NonZeroU64;
use std::sync::{Arc, Mutex, MutexGuard};

use ultragraphdb_core_ids::TypePrefix;

use crate::traits::block_source::BlockSource;
use crate::types::sequence_block::SequenceBlock;

/// A [`BlockSource`] for tests, standing in for the metadata group: each type's blocks follow one another from
/// sequence 1, each `block_len` long, unless the type was given scripted blocks, which it hands out in order and then
/// no more. It hands out nothing while unreachable, and records every block it leased. Its clones share one state, as
/// the partitions of one cluster share one metadata group, so a test keeps a handle on what a store leases from.
#[derive(Debug, Clone)]
pub struct TestBlockSource {
    // Shared: the clones a test hands its stores lease from one metadata group, and the test inspects and steers it.
    state: Arc<Mutex<State>>,
}

/// What a [`TestBlockSource`]'s clones share.
#[derive(Debug)]
struct State {
    block_len: NonZeroU64,
    /// The start of each type's next counted block; a type absent starts at 1.
    next: BTreeMap<u16, u64>,
    /// The blocks a scripted type has left to hand out.
    scripted: BTreeMap<u16, VecDeque<SequenceBlock>>,
    reachable: bool,
    leased: Vec<(TypePrefix, SequenceBlock)>,
}

impl TestBlockSource {
    /// A reachable source whose counted blocks hold `block_len` sequences each.
    #[must_use]
    pub fn new(block_len: NonZeroU64) -> Self {
        let state = State {
            block_len,
            next: BTreeMap::new(),
            scripted: BTreeMap::new(),
            reachable: true,
            leased: Vec::new(),
        };
        Self {
            state: Arc::new(Mutex::new(state)),
        }
    }

    /// The source handing out exactly `blocks` of `prefix`, in order, and then no block of it.
    #[must_use]
    pub fn with_blocks(self, prefix: TypePrefix, blocks: Vec<SequenceBlock>) -> Self {
        let _replaced: Option<VecDeque<SequenceBlock>> =
            self.lock().scripted.insert(prefix.get(), blocks.into());
        self
    }

    /// Makes the source reachable or not; an unreachable source leases nothing.
    pub fn set_reachable(&self, reachable: bool) {
        self.lock().reachable = reachable;
    }

    /// Every block of `prefix` leased so far, by any clone, in lease order.
    #[must_use]
    pub fn leased(&self, prefix: TypePrefix) -> Vec<SequenceBlock> {
        self.lock()
            .leased
            .iter()
            .filter(|(p, _)| *p == prefix)
            .map(|(_, block)| *block)
            .collect()
    }

    /// The shared state, a poisoned lock's included: every change to it is made whole before the guard drops.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl State {
    /// The next block of `prefix`: its next scripted block, or the counted block after the last, `None` once the
    /// script or the type's sequences are spent.
    fn next_block(&mut self, prefix: TypePrefix) -> Option<SequenceBlock> {
        if let Some(script) = self.scripted.get_mut(&prefix.get()) {
            return script.pop_front();
        }
        let start = self.next.get(&prefix.get()).copied().unwrap_or(1);
        let end = start.checked_add(self.block_len.get())?;
        let block = SequenceBlock::new(start, end)?;
        let _previous: Option<u64> = self.next.insert(prefix.get(), end);
        Some(block)
    }
}

impl BlockSource for TestBlockSource {
    fn lease(&self, prefix: TypePrefix) -> Option<SequenceBlock> {
        let mut state = self.lock();
        if !state.reachable {
            return None;
        }
        let block = state.next_block(prefix)?;
        debug_assert!(
            state.leased.iter().all(|(p, held)| *p != prefix
                || held.end() <= block.start().get()
                || block.end() <= held.start().get()),
            "a block is leased once"
        );
        state.leased.push((prefix, block));
        Some(block)
    }
}
