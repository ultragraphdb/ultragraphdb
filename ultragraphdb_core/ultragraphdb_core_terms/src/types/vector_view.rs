/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The elements of a vector literal as its encoding holds them, little-endian 32-bit floats, read in place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VectorView<'a>(&'a [u8]);

impl<'a> VectorView<'a> {
    /// The view of `bytes`, whose length is a multiple of four.
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }

    /// The number of elements.
    pub fn len(&self) -> usize {
        self.0.len() / 4
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The elements, in order.
    pub fn iter(&self) -> impl Iterator<Item = f32> + 'a {
        self.0
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
    }

    /// The elements' bytes, little-endian, as the encoding holds them.
    pub fn as_bytes(&self) -> &'a [u8] {
        self.0
    }
}
