/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, protos/pb.proto (UidBlock). Changed for ultragraphdb: a Rust type in place of the
 * protobuf message, stored in the repository's posting format.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// One block of a [`UidPack`](crate::UidPack): its first UID, the group-varint deltas of the others, and how many
/// UIDs it holds, the base included. All of a block's UIDs share their upper 32 bits.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UidBlock {
    base: u64,
    deltas: Vec<u8>,
    num_uids: u32,
}

impl UidBlock {
    /// The block starting at `base`, the others as `deltas`, `num_uids` UIDs in all.
    pub fn new(base: u64, deltas: Vec<u8>, num_uids: u32) -> Self {
        Self {
            base,
            deltas,
            num_uids,
        }
    }

    /// The block's first UID.
    pub fn base(&self) -> u64 {
        self.base
    }

    /// The deltas of the UIDs after the base, in group-varint groups of four.
    pub fn deltas(&self) -> &[u8] {
        &self.deltas
    }

    /// The number of UIDs in the block, the base included.
    pub fn num_uids(&self) -> u32 {
        self.num_uids
    }
}
