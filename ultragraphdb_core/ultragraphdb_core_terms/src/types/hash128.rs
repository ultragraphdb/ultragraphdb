/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use blake2::{Blake2b128, Digest};

/// The 128-bit hash naming a string's dictionary entry: BLAKE2b with a 16-byte digest. A keyed hash would need a
/// secret; BLAKE2b resists chosen collisions in untrusted input without one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash128([u8; 16]);

impl Hash128 {
    /// The hash of `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        Self(Blake2b128::digest(bytes).into())
    }

    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}
