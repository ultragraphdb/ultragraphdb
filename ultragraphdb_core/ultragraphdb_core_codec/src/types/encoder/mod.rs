/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, codec/codec.go. Changed for ultragraphdb: translated to Rust; blocks are allocated
 * on the heap rather than on ristretto's z.Allocator, so a pack needs no FreePack and its alloc_ref stays 0.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_proto_pb::pb::{UidBlock, UidPack};

use crate::codec::match32_msb;
use crate::groupvarint;

/// Converts a list of UIDs, added in ascending order, into a [`UidPack`].
#[derive(Debug, Clone)]
pub struct Encoder {
    block_size: usize,
    pack: Option<UidPack>,
    uids: Vec<u64>,
}

impl Encoder {
    /// An encoder closing a block at `block_size` UIDs. Dgraph's posting lists use 256; a block size of 0 packs one
    /// UID per block, as Go's does.
    pub fn new(block_size: usize) -> Self {
        Self {
            block_size,
            pack: None,
            uids: Vec::new(),
        }
    }

    pub fn block_size(&self) -> usize {
        self.block_size
    }

    /// Adds a UID to the list being encoded. A block closes when it holds the block size or when `uid`'s upper 32 bits
    /// differ from the previous UID's.
    pub fn add(&mut self, uid: u64) {
        if self.pack.is_none() {
            self.pack = Some(UidPack {
                block_size: self.block_size as u32,
                ..UidPack::default()
            });
        }

        if let Some(&last) = self.uids.last()
            && !match32_msb(last, uid)
        {
            self.pack_block();
        }

        self.uids.push(uid);
        if self.uids.len() >= self.block_size {
            self.pack_block();
        }
    }

    /// The encoded pack, or `None` when no UID was added, as Go's `Done` returns nil.
    pub fn done(mut self) -> Option<UidPack> {
        self.pack_block();
        self.pack
    }

    /// Closes the pending UIDs into a block: the first is the base, the rest go as deltas in groups of four, the last
    /// group padded with zeros. A block of one UID still emits one group.
    fn pack_block(&mut self) {
        let Some((&base, rest)) = self.uids.split_first() else {
            return;
        };

        let mut deltas = Vec::with_capacity(rest.len().div_ceil(4).max(1) * 17);
        let mut last = base;
        let mut group = |chunk: &[u64]| {
            let mut tmp = [0u32; 4];
            for (slot, &uid) in tmp.iter_mut().zip(chunk) {
                *slot = uid.wrapping_sub(last) as u32;
                last = uid;
            }
            groupvarint::encode4(&mut deltas, tmp);
        };
        if rest.is_empty() {
            group(&[]);
        } else {
            rest.chunks(4).for_each(group);
        }

        let block = UidBlock {
            base,
            deltas,
            num_uids: self.uids.len() as u32,
        };
        if let Some(pack) = self.pack.as_mut() {
            pack.blocks.push(block);
        }
        self.uids.clear();
    }
}
