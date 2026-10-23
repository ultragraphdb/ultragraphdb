/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, codec/codec.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_proto_pb::pb::UidPack;

use crate::groupvarint;
use crate::types::seek_pos::SeekPos;

/// Reads a [`UidPack`] back into UIDs, one block at a time.
///
/// The UIDs a method returns belong to the decoder and change on its next call.
#[derive(Debug, Clone)]
pub struct Decoder<'a> {
    pack: &'a UidPack,
    block_idx: usize,
    uids: Vec<u64>,
}

impl<'a> Decoder<'a> {
    /// A decoder holding the pack's first block: Go's `NewDecoder`.
    pub fn new(pack: &'a UidPack) -> Self {
        let mut decoder = Self::unpositioned(pack);
        decoder.seek(0, SeekPos::Start);
        decoder
    }

    /// A decoder at the first block with nothing unpacked yet, Go's `Decoder{Pack: pack}`: a seek positions it.
    pub fn unpositioned(pack: &'a UidPack) -> Self {
        Self {
            pack,
            block_idx: 0,
            uids: Vec::new(),
        }
    }

    pub fn pack(&self) -> &'a UidPack {
        self.pack
    }

    /// The index of the block being decoded.
    pub fn block_idx(&self) -> usize {
        self.block_idx
    }

    /// The UIDs of the current block, from the last seek's position on.
    pub fn uids(&self) -> &[u64] {
        &self.uids
    }

    /// Unpacks the current block, all of it, or nothing past the last block.
    pub fn unpack_block(&mut self) -> &[u64] {
        self.uids.clear();
        let Some(block) = self.pack.blocks.get(self.block_idx) else {
            return &self.uids;
        };

        let num_uids = block.num_uids as usize;
        let mut last = block.base;
        self.uids.push(last);

        let mut enc_data: &[u8] = &block.deltas;
        while self.uids.len() < num_uids {
            // Bytes past the end of the deltas read as zero, as Go pads them to 17 bytes.
            let tmp = groupvarint::decode4(enc_data);
            let used = groupvarint::bytes_used(enc_data.first().copied().unwrap_or(0));
            enc_data = enc_data.get(used..).unwrap_or_default();
            for delta in tmp {
                last = last.wrapping_add(u64::from(delta));
                self.uids.push(last);
            }
        }

        self.uids.truncate(num_uids);
        &self.uids
    }

    /// The number of UIDs the blocks from the current one on could hold.
    pub fn approx_len(&self) -> usize {
        self.pack.block_size as usize * self.pack.blocks.len().saturating_sub(self.block_idx)
    }

    /// Finds the block holding `uid` and unpacks it whole, which is faster than [`seek`](Self::seek) when the result
    /// is intersected anyway. The search starts at the current block unless `uid` lies before it.
    ///
    /// # Panics
    ///
    /// As Dgraph's does, when the decoder has moved past the last block, or when it was never positioned and `uid`
    /// lies inside the first block.
    pub fn seek_to_block(&mut self, uid: u64, whence: SeekPos) -> &[u64] {
        let mut prev_block_idx = self.block_idx;
        self.block_idx = 0;
        if uid == 0 {
            return self.unpack_block();
        }

        let blocks = &self.pack.blocks;
        // If for some reason we are searching an older uid, we need to search the entire pack. As in Dgraph, this
        // panics when the decoder has moved past the last block.
        if prev_block_idx > 0 && uid < blocks[prev_block_idx].base {
            prev_block_idx = 0;
        }

        let idx = prev_block_idx
            + blocks[prev_block_idx..].partition_point(|b| !whence.reached(b.base, uid));
        // The first block.Base >= uid.
        if idx == 0 {
            return self.unpack_block();
        }
        // The uid is the first entry in the block.
        if idx < blocks.len() && blocks[idx].base == uid {
            self.block_idx = idx;
            return self.unpack_block();
        }

        // Either idx is past the last block, or it is the first block whose base is greater than uid: in both cases
        // the previous block is where uid would be.
        self.block_idx = idx - 1;
        if self.block_idx != prev_block_idx {
            self.unpack_block();
        }

        // As in Dgraph, this panics when the block was not unpacked: a seek must position the decoder first.
        if uid <= self.uids[self.uids.len() - 1] {
            return &self.uids;
        }

        // No uid in the block is >= uid. The next block might still have valid entries > uid.
        self.next_block()
    }

    /// The UIDs of the block holding `uid`, from `uid` on (`Start`) or after it (`Current`); empty when no UID
    /// lies there. A seek of 0 returns the first block whole.
    pub fn seek(&mut self, uid: u64, whence: SeekPos) -> &[u64] {
        self.block_idx = 0;
        if uid == 0 {
            return self.unpack_block();
        }

        let blocks = &self.pack.blocks;
        let idx = blocks.partition_point(|b| !whence.reached(b.base, uid));
        // The first block.Base >= uid.
        if idx == 0 {
            return self.unpack_block();
        }
        // The uid is the first entry in the block.
        if idx < blocks.len() && blocks[idx].base == uid {
            self.block_idx = idx;
            return self.unpack_block();
        }

        // Either idx is past the last block, or it is the first block whose base is greater than uid: in both cases
        // the previous block is where uid would be.
        self.block_idx = idx - 1;
        self.unpack_block();

        // uidx is the first uid in the block at or past uid.
        let uidx = self.uids.partition_point(|&u| !whence.reached(u, uid));
        if uidx < self.uids.len() {
            self.uids.drain(..uidx);
            return &self.uids;
        }
        // No uid in the block is >= uid. The next block might still have valid entries > uid.
        self.next_block()
    }

    /// Moves forward to the last block whose base is at most `seek`, or stays at the current block when the next
    /// base is greater, and unpacks it. Go's version never returns for `u64::MAX`; this one stops past the last
    /// block and returns nothing.
    pub fn linear_seek(&mut self, seek: u64) -> &[u64] {
        while self.block_idx < self.pack.blocks.len() && seek >= self.peek_next_base() {
            self.block_idx += 1;
        }

        self.unpack_block()
    }

    /// The base of the next block without advancing, or `u64::MAX` past the last one.
    pub fn peek_next_base(&self) -> u64 {
        self.pack
            .blocks
            .get(self.block_idx + 1)
            .map_or(u64::MAX, |b| b.base)
    }

    /// Whether the decoder has not moved past the last block.
    pub fn valid(&self) -> bool {
        self.block_idx < self.pack.blocks.len()
    }

    /// Moves on to the next block and unpacks it: Go's `Next`.
    pub fn next_block(&mut self) -> &[u64] {
        self.block_idx += 1;
        self.unpack_block()
    }
}
