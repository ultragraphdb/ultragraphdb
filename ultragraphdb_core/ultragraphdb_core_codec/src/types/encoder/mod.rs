/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, codec/codec.go (Encoder). Changed for ultragraphdb: translated to Rust; system IDs in
 * place of UIDs, refused out of order; a block closes at the block size alone and stores LEB128 deltas of the IDs'
 * first 104 bits in place of group-varint u32 deltas.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_ids::SystemId;

use crate::errors::codec_error::CodecError;
use crate::types::uid_block::UidBlock;
use crate::types::uid_pack::UidPack;
use crate::utils::deltas;

/// Converts a list of system IDs, added in strictly increasing order, into a [`UidPack`].
#[derive(Debug, Clone)]
pub struct Encoder {
    block_size: usize,
    pack: Option<UidPack>,
    ids: Vec<SystemId>,
    added: usize,
}

impl Encoder {
    /// An encoder closing a block at `block_size` IDs. The posting lists use 256; a block size of 0 packs one ID per
    /// block, as Go's does.
    #[must_use]
    pub fn new(block_size: usize) -> Self {
        Self {
            block_size,
            pack: None,
            ids: Vec::with_capacity(block_size.clamp(1, 256)),
            added: 0,
        }
    }

    /// The number of IDs at which a block closes.
    #[must_use]
    pub fn block_size(&self) -> usize {
        self.block_size
    }

    /// Adds an ID to the list being encoded; a block closes when it holds the block size.
    ///
    /// # Errors
    ///
    /// [`CodecError::out_of_order`] naming the ID's position among those added, from 0, when it is not greater than
    /// the ID added before it; the encoder is unchanged.
    pub fn add(&mut self, id: SystemId) -> Result<(), CodecError> {
        let last = self.ids.last().copied().or_else(|| {
            self.pack
                .as_ref()
                .and_then(|p| p.blocks().last())
                .map(UidBlock::last)
        });
        if last.is_some_and(|last| id <= last) {
            return Err(CodecError::out_of_order(self.added));
        }
        let block_size = u32::try_from(self.block_size).unwrap_or(u32::MAX);
        let pack = self.pack.get_or_insert_with(|| UidPack::empty(block_size));
        debug_assert_eq!(
            pack.block_size(),
            block_size,
            "the pack takes the encoder's block size"
        );
        self.ids.push(id);
        // One per ID held in memory, so it never saturates.
        self.added = self.added.saturating_add(1);
        if self.ids.len() >= self.block_size {
            self.pack_block();
        }
        Ok(())
    }

    /// The encoded pack, or `None` when no ID was added, as Go's `Done` returns nil.
    #[must_use]
    pub fn done(mut self) -> Option<UidPack> {
        self.pack_block();
        debug_assert!(
            self.pack.is_some() == (self.added > 0),
            "a pack exactly when an ID was added"
        );
        self.pack
    }

    /// Closes the pending IDs into a block: the first whole, each later one as its LEB128 delta.
    fn pack_block(&mut self) {
        let (Some(&first), Some(&last)) = (self.ids.first(), self.ids.last()) else {
            return;
        };
        let mut block_deltas =
            Vec::with_capacity(self.ids.len().saturating_sub(1).saturating_mul(4));
        for pair in self.ids.windows(2) {
            if let [previous, next] = pair {
                deltas::put(&mut block_deltas, *previous, *next);
            }
        }
        let count = u32::try_from(self.ids.len()).unwrap_or(u32::MAX);
        if let Some(pack) = self.pack.as_mut() {
            pack.push_block(UidBlock::from_encoder(first, block_deltas, count, last));
        }
        self.ids.clear();
    }
}
