/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::chunk_layout::ChunkLayout;
use crate::utils::checksum::{xxh3_64, xxh3_64_with_seed};
use crate::utils::constants::LEAF_SIZE;
use crate::utils::le::bytes_at;

/// A body whose chunks were checked against `checksum_body`: each chunk's leaf is XXH3-64 of the chunk seeded with
/// its index, `checksum_body` is XXH3-64 of the table of leaves with seed 0, or the one leaf of a body of one chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkedBody<'a> {
    data: &'a [u8],
    layout: ChunkLayout,
}

impl<'a> ChunkedBody<'a> {
    /// `body`, table included, checked against `checksum_body`: its length split into chunks and table, refused as
    /// [`ProtocolError::BodyLength`]; then the table against `checksum_body`, refused as
    /// [`ProtocolError::ChunkTable`]; then each chunk in order against its leaf, the first failing refused as
    /// [`ProtocolError::Chunk`] with its index and byte range. A body of one chunk has no table, and its chunk is
    /// checked against `checksum_body` directly.
    pub fn verify(body: &'a [u8], checksum_body: u64) -> Result<Self, ProtocolError> {
        let layout = ChunkLayout::of_body(body.len())?;
        let (data, table) = body.split_at(layout.data_len());
        if layout.chunks() == 1 {
            if xxh3_64_with_seed(data, 0) != checksum_body {
                return Err(ProtocolError::Chunk(0, 0, data.len()));
            }
            return Ok(Self { data, layout });
        }
        let computed = xxh3_64(table);
        if computed != checksum_body {
            return Err(ProtocolError::ChunkTable(checksum_body, computed));
        }
        for index in 0..layout.chunks() {
            let range = layout.chunk_range(index).unwrap_or_default();
            let stored = bytes_at(table, index * LEAF_SIZE).map(u64::from_le_bytes);
            let chunk = data.get(range.clone()).unwrap_or_default();
            if stored != Some(xxh3_64_with_seed(chunk, index as u64)) {
                return Err(ProtocolError::Chunk(index, range.start, range.end));
            }
        }
        Ok(Self { data, layout })
    }

    /// Appends `data` and, when it takes more than one chunk, its chunk table to `out`, and returns `checksum_body`.
    pub fn encode(data: &[u8], out: &mut Vec<u8>) -> u64 {
        let layout = ChunkLayout::of_data(data.len());
        out.extend_from_slice(data);
        if layout.chunks() == 1 {
            return xxh3_64_with_seed(data, 0);
        }
        let start = out.len();
        for index in 0..layout.chunks() {
            let range = layout.chunk_range(index).unwrap_or_default();
            let leaf = xxh3_64_with_seed(data.get(range).unwrap_or_default(), index as u64);
            out.extend_from_slice(&leaf.to_le_bytes());
        }
        xxh3_64(out.get(start..).unwrap_or_default())
    }

    /// The bytes the chunks cover, the body without its table.
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    pub fn layout(&self) -> ChunkLayout {
        self.layout
    }
}
