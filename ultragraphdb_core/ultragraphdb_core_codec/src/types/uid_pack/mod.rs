/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod uid_pack_codec;

use crate::errors::codec_error::CodecError;
use crate::types::uid_block::UidBlock;

/// A strictly increasing list of system IDs packed into blocks of at most `block_size` IDs, each block's IDs above
/// the previous block's: the repository's own format, in place of Dgraph's `UidPack`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UidPack {
    block_size: u32,
    blocks: Vec<UidBlock>,
}

impl UidPack {
    /// The format version a pack's bytes begin with.
    pub const VERSION: u8 = 1;

    /// The pack of `blocks`, each holding at most `block_size` IDs.
    ///
    /// # Errors
    ///
    /// [`CodecError::blocks_out_of_order`] naming the first block whose first ID is not above the previous block's
    /// last, or its first when that block is empty.
    pub fn new(block_size: u32, blocks: Vec<UidBlock>) -> Result<Self, CodecError> {
        if let Some(block) = blocks
            .windows(2)
            .position(|pair| matches!(pair, [before, after] if after.first() <= before.last()))
        {
            return Err(CodecError::blocks_out_of_order(block + 1));
        }
        Ok(Self { block_size, blocks })
    }

    /// The most IDs a block holds.
    #[must_use]
    pub fn block_size(&self) -> u32 {
        self.block_size
    }

    /// The blocks, in ID order.
    #[must_use]
    pub fn blocks(&self) -> &[UidBlock] {
        &self.blocks
    }

    /// The pack of no blocks yet, for the encoder to fill.
    pub(crate) fn empty(block_size: u32) -> Self {
        Self {
            block_size,
            blocks: Vec::new(),
        }
    }

    /// Appends a block the encoder closed, above every block before it.
    pub(crate) fn push_block(&mut self, block: UidBlock) {
        debug_assert!(
            self.blocks
                .last()
                .is_none_or(|before| before.last() < block.first()),
            "a block starts above the block before it"
        );
        self.blocks.push(block);
    }
}
