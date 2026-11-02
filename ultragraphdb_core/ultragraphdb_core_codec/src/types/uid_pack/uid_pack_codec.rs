/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A pack's bytes, little-endian but for the IDs:
//!
//! - the format version, 1 ([`UidPack::VERSION`]);
//! - the block size, u32, and the block count, u32;
//! - per block: its first ID as its 16 big-endian bytes, its ID count, u32, and its LEB128 deltas as a u32 length and
//!   the bytes.
//!
//! A list has one encoding: every field is fixed and every delta is canonical LEB128, so a pack the decoder accepts
//! encodes back to its bytes.

use ultragraphdb_core_ids::SystemId;

use crate::errors::codec_error::CodecError;
use crate::types::uid_block::UidBlock;
use crate::types::uid_pack::UidPack;

/// The bytes before the blocks: the version, the block size and the block count.
const HEAD_LEN: usize = 1 + 4 + 4;
/// A block's fixed fields: its first ID, its ID count and its deltas' length.
const BLOCK_LEN: usize = SystemId::BYTES + 4 + 4;

/// Writes a count or a length; a pack holds fewer than 2^32 blocks and a block fewer than 2^32 bytes of deltas.
fn put_u32(w: &mut Vec<u8>, n: usize) {
    debug_assert!(u32::try_from(n).is_ok(), "a count or length fits 32 bits");
    w.extend_from_slice(&u32::try_from(n).unwrap_or(u32::MAX).to_le_bytes());
}

/// Reads a pack's fields front to back, `at` the next byte of `bytes`.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], CodecError> {
        let field = self
            .at
            .checked_add(n)
            .and_then(|end| self.bytes.get(self.at..end))
            .ok_or_else(|| CodecError::truncated(self.at))?;
        self.at += n;
        Ok(field)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], CodecError> {
        let at = self.at;
        <[u8; N]>::try_from(self.take(N)?).map_err(|_| CodecError::truncated(at))
    }

    fn u32(&mut self) -> Result<u32, CodecError> {
        Ok(u32::from_le_bytes(self.array::<4>()?))
    }

    fn block(&mut self) -> Result<UidBlock, CodecError> {
        let first = SystemId::decode(&self.array::<{ SystemId::BYTES }>()?)
            .map_err(|e| CodecError::invalid_id(0, e))?;
        let count = self.u32()?;
        let len = usize::try_from(self.u32()?).map_err(|_| CodecError::truncated(self.at))?;
        let deltas = self.take(len)?;
        UidBlock::new(first, deltas.to_vec(), count)
    }
}

impl UidPack {
    /// The pack's bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Vec::with_capacity(self.encoded_len());
        self.encode_into(&mut w);
        w
    }

    /// Appends the pack's bytes to `w`, as [`UidPack::encode`] gives them.
    pub fn encode_into(&self, w: &mut Vec<u8>) {
        let start = w.len();
        w.push(Self::VERSION);
        w.extend_from_slice(&self.block_size.to_le_bytes());
        put_u32(w, self.blocks.len());
        for block in &self.blocks {
            w.extend_from_slice(&block.first().to_be_bytes());
            w.extend_from_slice(&block.count().to_le_bytes());
            put_u32(w, block.deltas().len());
            w.extend_from_slice(block.deltas());
        }
        debug_assert_eq!(
            w.len() - start,
            self.encoded_len(),
            "the length is the bytes'"
        );
    }

    /// The number of bytes [`UidPack::encode`] writes, without writing them.
    #[must_use]
    pub fn encoded_len(&self) -> usize {
        HEAD_LEN
            + self
                .blocks
                .iter()
                .map(|b| BLOCK_LEN + b.deltas().len())
                .sum::<usize>()
    }

    /// The pack whose bytes are `bytes`, all of them.
    ///
    /// # Errors
    ///
    /// [`CodecError::truncated`] when the bytes end inside a field, [`CodecError::unknown_version`] for a version
    /// other than [`UidPack::VERSION`], [`CodecError::trailing_bytes`] for bytes after the last block, and the
    /// refusals of [`UidBlock::new`](crate::UidBlock::new) and [`UidPack::new`] for a block's IDs and the blocks'
    /// order: nothing is decoded in part.
    pub fn decode(bytes: &[u8]) -> Result<UidPack, CodecError> {
        let (pack, end) = Self::read_at(bytes, 0)?;
        if end != bytes.len() {
            return Err(CodecError::trailing_bytes(end));
        }
        Ok(pack)
    }

    /// The pack whose bytes start at `at` in `bytes`, and the byte after them; an offset an error names counts from
    /// the start of `bytes`.
    pub(crate) fn read_at(bytes: &[u8], at: usize) -> Result<(UidPack, usize), CodecError> {
        let mut r = Reader { bytes, at };
        let [version] = r.array::<1>()?;
        if version != Self::VERSION {
            return Err(CodecError::unknown_version(version));
        }
        let block_size = r.u32()?;
        let count = usize::try_from(r.u32()?).map_err(|_| CodecError::truncated(r.at))?;
        // Each block takes at least BLOCK_LEN bytes, so the bytes bound the allocation whatever the count says.
        let mut blocks =
            Vec::with_capacity(count.min(bytes.len().saturating_sub(r.at) / BLOCK_LEN));
        for _ in 0..count {
            blocks.push(r.block()?);
        }
        let pack = UidPack::new(block_size, blocks)?;
        debug_assert_eq!(
            r.at - at,
            pack.encoded_len(),
            "the pack took the bytes it encodes to"
        );
        Ok((pack, r.at))
    }
}
