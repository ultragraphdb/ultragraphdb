/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The elements of a `ugdb:bfloat16vector` as its encoding holds them (`PROTOCOL.md` §10.1, tag `0x41`), little-endian
/// bfloat16s, each the high 16 bits of an IEEE 754 binary32, read in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Bfloat16VectorView<'a>(&'a [u8]);

impl<'a> Bfloat16VectorView<'a> {
    /// The view of `bytes`, whose length is a multiple of two.
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }

    /// The number of elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len() / 2
    }

    /// Whether it has no elements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The elements, in order, each the binary32 whose high 16 bits it is.
    pub fn iter(&self) -> impl Iterator<Item = f32> + 'a {
        self.0
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| f32::from_bits(u32::from(u16::from_le_bytes(*c)) << 16))
    }

    /// The elements' bytes, little-endian, as the encoding holds them.
    #[must_use]
    pub fn as_bytes(&self) -> &'a [u8] {
        self.0
    }
}
