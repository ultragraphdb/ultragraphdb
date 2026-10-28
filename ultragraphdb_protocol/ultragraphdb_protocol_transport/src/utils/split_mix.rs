/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// SplitMix64 (Steele, Lea and Flood, "Fast splittable pseudorandom number generators", OOPSLA 2014): the network's
/// one stream of draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SplitMix(u64);

impl SplitMix {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        // SplitMix64's constants, from the paper and Vigna's reference splitmix64.c.
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Whether an event of `per_million` happens.
    pub(crate) fn chance(&mut self, per_million: u32) -> bool {
        self.below(1_000_000) < u64::from(per_million)
    }

    /// A draw in `0..n`, zero when `n` is zero.
    pub(crate) fn below(&mut self, n: u64) -> u64 {
        match n {
            0 => 0,
            n => self.next_u64() % n,
        }
    }
}
