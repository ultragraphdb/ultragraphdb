/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;

use crate::errors::codec_error::CodecError;
use crate::utils::deltas;

/// One block of a [`UidPack`](crate::UidPack): its first ID, the LEB128 deltas of the others, and how many IDs it
/// holds, the first included. Each delta is the difference of an ID's first 104 bits (type, sequence and bucket) from
/// the previous ID's, at least 1, so a block's IDs strictly increase. A block of 0 IDs holds no deltas and contributes
/// nothing to a decode; its first ID orders it among the blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UidBlock {
    first: SystemId,
    deltas: Vec<u8>,
    count: u32,
    last: SystemId,
}

impl UidBlock {
    /// The block starting at `first`, the others as `deltas`, `count` IDs in all; the deltas are read in full, so a
    /// block exists only when every ID it holds is valid.
    ///
    /// # Errors
    ///
    /// Each names the position of the ID or the byte of the deltas at fault: [`CodecError::truncated_varint`] when
    /// the deltas end before `count - 1` of them are read, [`CodecError::overflowing_varint`] for a delta longer than
    /// 15 bytes, [`CodecError::non_canonical_varint`] for one with a redundant last group,
    /// [`CodecError::out_of_order`] for a delta of 0, [`CodecError::delta_overflow`] for one carrying an ID past 104
    /// bits, [`CodecError::invalid_id`] for an ID that is not a valid system ID, and [`CodecError::trailing_deltas`]
    /// for bytes after the last delta.
    pub fn new(first: SystemId, deltas: Vec<u8>, count: u32) -> Result<Self, CodecError> {
        let last = deltas::walk(first, &deltas, count, |_| {})?;
        Ok(Self {
            first,
            deltas,
            count,
            last,
        })
    }

    /// The block's first ID.
    #[must_use]
    pub fn first(&self) -> SystemId {
        self.first
    }

    /// The LEB128 deltas of the IDs after the first.
    #[must_use]
    pub fn deltas(&self) -> &[u8] {
        &self.deltas
    }

    /// The number of IDs in the block, the first included.
    #[must_use]
    pub fn count(&self) -> u32 {
        self.count
    }

    /// The block's last ID, or its first when it is empty: what the next block's first ID is above.
    pub(crate) fn last(&self) -> SystemId {
        self.last
    }

    /// The block of `first`, `deltas`, `count` and `last` as the encoder built them, valid by construction.
    pub(crate) fn from_encoder(
        first: SystemId,
        deltas: Vec<u8>,
        count: u32,
        last: SystemId,
    ) -> Self {
        debug_assert_eq!(
            deltas::walk(first, &deltas, count, |_| {}),
            Ok(last),
            "the encoder's block reads back"
        );
        Self {
            first,
            deltas,
            count,
            last,
        }
    }
}
