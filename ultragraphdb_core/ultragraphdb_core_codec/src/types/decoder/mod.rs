/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, codec/codec.go (Decoder). Changed for ultragraphdb: translated to Rust; system IDs in
 * place of UIDs, sought by any 128-bit target; a decoder is positioned when made; SeekToBlock searches from the last
 * block past the end and from the target's block after a backward seek, LinearSeek returns for the largest target,
 * and an empty block contributes nothing, where Go's panics or loops (ledger 3, 4, 5, 27).
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_ids::SystemId;

use crate::types::seek_pos::SeekPos;
use crate::types::uid_block::UidBlock;
use crate::types::uid_pack::UidPack;
use crate::utils::deltas;

/// Reads a [`UidPack`] back into system IDs, one block at a time.
///
/// A decoder exists only once positioned: it always holds the unpack of its current block, from the last seek's
/// position on. The IDs a method returns belong to the decoder and change on its next call. A target is any 128-bit
/// value, a valid system ID or not, compared with the IDs' 128-bit values.
#[derive(Debug, Clone)]
pub struct Decoder<'a> {
    pack: &'a UidPack,
    block_idx: usize,
    ids: Vec<SystemId>,
}

impl<'a> Decoder<'a> {
    /// A decoder holding the pack's first block: Go's `NewDecoder`, and `sought(pack, 0, Start)`.
    #[must_use]
    pub fn new(pack: &'a UidPack) -> Self {
        Self::sought(pack, 0, SeekPos::Start)
    }

    /// A decoder positioned by [`seek`](Self::seek)ing `target` with `whence`: Go's `Decoder{Pack: pack}` followed by
    /// `Seek`, in one step, so that no decoder exists without the unpack of its current block.
    #[must_use]
    pub fn sought(pack: &'a UidPack, target: u128, whence: SeekPos) -> Self {
        let block_size = usize::try_from(pack.block_size()).unwrap_or(usize::MAX);
        let mut decoder = Self {
            pack,
            block_idx: 0,
            ids: Vec::with_capacity(block_size.clamp(1, 256)),
        };
        // The decoder holds the block it sought, so the returned view of it is not needed here.
        let _sought_block: &[SystemId] = decoder.seek(target, whence);
        decoder
    }

    /// The pack being decoded.
    #[must_use]
    pub fn pack(&self) -> &'a UidPack {
        self.pack
    }

    /// The index of the block being decoded.
    #[must_use]
    pub fn block_idx(&self) -> usize {
        self.block_idx
    }

    /// The IDs of the current block, from the last seek's position on.
    #[must_use]
    pub fn ids(&self) -> &[SystemId] {
        &self.ids
    }

    /// Unpacks the current block, all of it, or nothing past the last block.
    pub fn unpack_block(&mut self) -> &[SystemId] {
        self.unpack();
        &self.ids
    }

    /// Unpacks the current block into `ids`, for the callers that read it from there.
    fn unpack(&mut self) {
        self.ids.clear();
        let Some(block) = self.pack.blocks().get(self.block_idx) else {
            return;
        };
        let ids = &mut self.ids;
        let walked = deltas::walk(block.first(), block.deltas(), block.count(), |id| {
            ids.push(id)
        });
        // A block exists only once its deltas have been read in full, so the walk cannot fail; were it to, the block
        // contributes nothing rather than a part of it.
        debug_assert!(walked.is_ok(), "a block's deltas read back");
        if walked.is_err() {
            self.ids.clear();
        }
    }

    /// Moves on from an empty current block to the next block holding IDs, or past the last block: an empty block
    /// contributes nothing.
    fn step_over_empty_blocks(&mut self) {
        let blocks = self.pack.blocks().len();
        // At most one pass per block: each moves block_idx forward, and the loop ends past the last block.
        while self.ids.is_empty() && self.block_idx < blocks {
            // Below the block count, so it never saturates.
            self.block_idx = self.block_idx.saturating_add(1);
            self.unpack();
        }
    }

    /// The number of IDs the blocks from the current one on could hold.
    #[must_use]
    pub fn approx_len(&self) -> usize {
        let block_size = usize::try_from(self.pack.block_size()).unwrap_or(usize::MAX);
        block_size.saturating_mul(self.pack.blocks().len().saturating_sub(self.block_idx))
    }

    /// Finds the block holding `target` and unpacks it whole, which is faster than [`seek`](Self::seek) when the
    /// result is intersected anyway. The search starts at the current block unless `target` lies before it; a decoder
    /// past the last block searches from the last block. Empty when no ID lies at or after `target`, and an empty
    /// block contributes nothing.
    ///
    /// Dgraph's panics past the last block (ledger 3) and on an empty block (4), and after a backward seek into the
    /// first block returns the block it held (27); the port returns the target's block.
    pub fn seek_to_block(&mut self, target: u128, whence: SeekPos) -> &[SystemId] {
        let held_block_idx = self.block_idx;
        let blocks = self.pack.blocks();
        // A target before the held block's first ID searches the whole pack.
        let from = if blocks
            .get(held_block_idx)
            .is_some_and(|b| target < b.first().to_u128())
        {
            0
        } else {
            held_block_idx.min(blocks.len())
        };
        // At most the blocks after `from`, so the sum never saturates.
        let idx = from.saturating_add(blocks.get(from..).map_or(0, |rest| {
            rest.partition_point(|b| !whence.reached(b.first().to_u128(), target))
        }));
        debug_assert!(idx <= blocks.len(), "the search stays within the blocks");
        // The target is a block's first ID, or before the first block: that block, whole.
        if target == 0
            || idx == 0
            || blocks
                .get(idx)
                .is_some_and(|b| b.first().to_u128() == target)
        {
            self.block_idx = if target == 0 { 0 } else { idx };
            self.unpack();
            self.step_over_empty_blocks();
            return &self.ids;
        }
        // Either idx is past the last block, or it is the first block whose first ID is past the target: in both
        // cases the previous block is where the target would be.
        // The block is unpacked again even when it is the one held, which a seek may have cut to its tail. idx is at
        // least 1 here.
        self.block_idx = idx.saturating_sub(1);
        self.unpack();
        if self.ids.last().is_some_and(|last| target <= last.to_u128()) {
            return &self.ids;
        }
        // No ID in the block is at or past the target. The next block might still hold some.
        self.next_block()
    }

    /// The IDs of the block holding `target`, from `target` on (`Start`) or after it (`Current`); empty when no ID
    /// lies there. A seek of 0 returns the first block whole.
    pub fn seek(&mut self, target: u128, whence: SeekPos) -> &[SystemId] {
        let blocks = self.pack.blocks();
        let idx = blocks.partition_point(|b| !whence.reached(b.first().to_u128(), target));
        // The target is a block's first ID, or before the first block: that block, whole.
        if target == 0
            || idx == 0
            || blocks
                .get(idx)
                .is_some_and(|b| b.first().to_u128() == target)
        {
            self.block_idx = if target == 0 { 0 } else { idx };
            self.unpack();
            self.step_over_empty_blocks();
            return &self.ids;
        }
        // Either idx is past the last block, or it is the first block whose first ID is past the target: in both
        // cases the previous block is where the target would be. idx is at least 1 here.
        self.block_idx = idx.saturating_sub(1);
        self.unpack();
        // The first ID in the block at or past the target.
        let at = self
            .ids
            .partition_point(|id| !whence.reached(id.to_u128(), target));
        if at < self.ids.len() {
            drop(self.ids.drain(..at));
            return &self.ids;
        }
        // No ID in the block is at or past the target. The next block might still hold some.
        self.next_block()
    }

    /// Moves forward to the last block whose first ID is at most `target`, or stays at the current block when the
    /// next block's first ID is greater, and unpacks it, moving on from an empty block. For `u128::MAX` it stays on
    /// the last block, as Go's documentation states; Go's version never returns for its largest target (ledger 5).
    pub fn linear_seek(&mut self, target: u128) -> &[SystemId] {
        // At most one pass per block: each moves block_idx forward, and peek_next_first ends at the last block.
        while let Some(next) = self.peek_next_first()
            && target >= next.to_u128()
        {
            // A next block exists, so it never saturates.
            self.block_idx = self.block_idx.saturating_add(1);
        }
        self.unpack();
        self.step_over_empty_blocks();
        &self.ids
    }

    /// The first ID of the next block without advancing, or `None` at or past the last one.
    #[must_use]
    pub fn peek_next_first(&self) -> Option<SystemId> {
        self.pack
            .blocks()
            .get(self.block_idx.saturating_add(1))
            .map(UidBlock::first)
    }

    /// Whether the decoder has not moved past the last block.
    #[must_use]
    pub fn valid(&self) -> bool {
        self.block_idx < self.pack.blocks().len()
    }

    /// Moves on to the next block and unpacks it, stepping over an empty block: Go's `Next`. Past the last block it
    /// holds nothing.
    pub fn next_block(&mut self) -> &[SystemId] {
        self.block_idx = self
            .block_idx
            .saturating_add(1)
            .min(self.pack.blocks().len());
        self.unpack();
        self.step_over_empty_blocks();
        &self.ids
    }
}
