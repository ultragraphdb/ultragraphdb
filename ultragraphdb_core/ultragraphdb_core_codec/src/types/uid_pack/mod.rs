/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, protos/pb.proto (UidPack). Changed for ultragraphdb: a Rust type in place of the
 * protobuf message, stored in the repository's posting format; `alloc_ref`, which only Dgraph's allocator reads, is
 * not carried.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::uid_block::UidBlock;

/// A sorted UID list packed into blocks of at most `block_size` UIDs: Dgraph's `UidPack`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UidPack {
    block_size: u32,
    blocks: Vec<UidBlock>,
}

impl UidPack {
    /// The pack of `blocks`, each holding at most `block_size` UIDs.
    pub fn new(block_size: u32, blocks: Vec<UidBlock>) -> Self {
        Self { block_size, blocks }
    }

    /// The most UIDs a block holds.
    pub fn block_size(&self) -> u32 {
        self.block_size
    }

    /// The blocks, in UID order.
    pub fn blocks(&self) -> &[UidBlock] {
        &self.blocks
    }

    /// Appends a block, as the encoder closes it.
    pub(crate) fn push_block(&mut self, block: UidBlock) {
        self.blocks.push(block);
    }
}
