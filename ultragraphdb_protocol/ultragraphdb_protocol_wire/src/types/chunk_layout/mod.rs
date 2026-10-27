/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::ops::Range;

use ultragraphdb_error_protocol::ProtocolError;

use crate::utils::constants::{CHUNK_SIZE, FIRST_CHUNK_SIZE, LEAF_SIZE};

/// How a body splits into chunks and its chunk table. Chunk boundaries fall on the frame's 4,096-byte blocks: chunk 0
/// is the body's first 3,840 bytes, the rest of the header's block, and every later chunk 4,096 bytes, the last
/// possibly shorter. A body of more than one chunk ends with its table of 8-byte leaves, and the chunks cover the
/// bytes before it; a body of at most one chunk, an empty one included, carries no table.
///
/// Byte ranges are offsets within the body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChunkLayout {
    data_len: usize,
    chunks: usize,
}

impl ChunkLayout {
    /// The layout of a body whose chunks hold `data_len` bytes, as a sender writes it.
    pub fn of_data(data_len: usize) -> Self {
        let chunks = match data_len.checked_sub(FIRST_CHUNK_SIZE) {
            None | Some(0) => 1,
            Some(rest) => 1 + rest.div_ceil(CHUNK_SIZE),
        };
        Self { data_len, chunks }
    }

    /// The layout of a received body of `body_len` bytes, table included: the one split whose chunk count and table
    /// add up to `body_len`, refused as [`ProtocolError::BodyLength`] when there is none.
    pub fn of_body(body_len: usize) -> Result<Self, ProtocolError> {
        if body_len <= FIRST_CHUNK_SIZE {
            return Ok(Self::of_data(body_len));
        }
        // n ≥ 2 chunks hold more than 3,840 + 4,096 (n − 2) bytes and at most 3,840 + 4,096 (n − 1); with their table
        // of 8n bytes, the body is 4,104n − 4,351 to 4,104n − 256 bytes. Those ranges for successive n do not meet,
        // so the one candidate is the least n whose range reaches body_len, ⌈(body_len + 256) / 4,104⌉, and it is
        // the split when the bytes before its table take n chunks.
        let refused = || ProtocolError::BodyLength(body_len);
        let chunks = body_len
            .checked_add(CHUNK_SIZE - FIRST_CHUNK_SIZE)
            .ok_or_else(refused)?
            .div_ceil(CHUNK_SIZE + LEAF_SIZE)
            .max(2);
        let layout = Self::of_data(body_len - LEAF_SIZE * chunks);
        if layout.chunks == chunks {
            Ok(layout)
        } else {
            Err(refused())
        }
    }

    /// The number of chunks, at least one: an empty body is one empty chunk.
    pub fn chunks(&self) -> usize {
        self.chunks
    }

    /// The bytes the chunks cover, the body before its table.
    pub fn data_len(&self) -> usize {
        self.data_len
    }

    /// The table's length: 8 bytes a chunk, or none for a body of one chunk.
    pub fn table_len(&self) -> usize {
        if self.chunks > 1 {
            LEAF_SIZE * self.chunks
        } else {
            0
        }
    }

    /// The body's length, chunks and table.
    pub fn body_len(&self) -> usize {
        self.data_len.saturating_add(self.table_len())
    }

    /// The body's bytes chunk `index` covers, `None` past the last chunk.
    pub fn chunk_range(&self, index: usize) -> Option<Range<usize>> {
        if index >= self.chunks {
            return None;
        }
        let start = if index == 0 {
            0
        } else {
            FIRST_CHUNK_SIZE + CHUNK_SIZE * (index - 1)
        };
        let end = if index == 0 {
            FIRST_CHUNK_SIZE
        } else {
            start.saturating_add(CHUNK_SIZE)
        };
        Some(start..end.min(self.data_len))
    }
}
