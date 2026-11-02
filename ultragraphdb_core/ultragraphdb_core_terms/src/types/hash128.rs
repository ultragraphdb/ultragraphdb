/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use sha2::{Digest, Sha256};

/// The 128-bit hash naming a string's dictionary entry: SHA-256 truncated to its first 16 bytes. A keyed hash would
/// need a secret; SHA-256 resists chosen collisions in untrusted input without one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash128([u8; 16]);

impl Hash128 {
    /// The hash of `bytes`.
    pub fn of(bytes: &[u8]) -> Self {
        let mut out = [0u8; 16];
        out.copy_from_slice(&Sha256::digest(bytes)[..16]);
        Self(out)
    }

    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}
