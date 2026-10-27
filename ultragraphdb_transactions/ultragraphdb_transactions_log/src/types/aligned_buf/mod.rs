/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod aligned_buf_clone;

/// Bytes whose first byte sits at a multiple of 4,096 in memory, as direct I/O requires of a buffer. The bytes live
/// inside a vector allocated 4,095 bytes larger, at the offset where it is aligned; the vector never grows, so the
/// bytes never move. A clone allocates and aligns its own.
#[derive(Debug)]
pub struct AlignedBuf {
    bytes: Vec<u8>,
    start: usize,
    len: usize,
}

impl AlignedBuf {
    /// The alignment of the bytes in memory, and of every offset and length an aligned file takes.
    pub const ALIGNMENT: usize = 4_096;

    /// `len` zero bytes.
    pub fn zeroed(len: usize) -> Self {
        let bytes = vec![0u8; len.saturating_add(Self::ALIGNMENT - 1)];
        // A u8 pointer can always be aligned, so the offset is below the alignment; clamped all the same.
        let start = bytes
            .as_ptr()
            .align_offset(Self::ALIGNMENT)
            .min(Self::ALIGNMENT - 1);
        Self { bytes, start, len }
    }

    /// A copy of `bytes`.
    pub fn from_slice(bytes: &[u8]) -> Self {
        let mut buf = Self::zeroed(bytes.len());
        buf.as_mut_slice().copy_from_slice(bytes);
        buf
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn as_slice(&self) -> &[u8] {
        self.bytes
            .get(self.start..self.start + self.len)
            .unwrap_or_default()
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        let range = self.start..self.start + self.len;
        self.bytes.get_mut(range).unwrap_or_default()
    }

    /// The vector and the range of it the bytes take, for an I/O call that takes the vector by value.
    pub(crate) fn into_parts(self) -> (Vec<u8>, usize, usize) {
        (self.bytes, self.start, self.len)
    }

    /// The bytes `start..start + len` of `bytes`, as [`AlignedBuf::into_parts`] gave them.
    pub(crate) fn from_parts(bytes: Vec<u8>, start: usize, len: usize) -> Self {
        Self { bytes, start, len }
    }
}
