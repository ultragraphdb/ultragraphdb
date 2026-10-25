/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A deterministic generator: SplitMix64 (Steele, Lea and Flood, 2014), which passes BigCrush and needs one word
/// of state. Every random choice of a simulation is drawn from one of these, never from an ambient source.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SimRng {
    state: u64,
}

impl SimRng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..n`, or 0 when `n` is 0.
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next_u64() % n }
    }

    /// An index into a collection of `len` elements, or 0 when it is empty.
    pub fn index(&mut self, len: usize) -> usize {
        self.below(len as u64) as usize
    }

    /// A number in `lo..=hi`, or `lo` when `hi` is below it. A range of every `u64` is the next output itself.
    pub fn between(&mut self, lo: u64, hi: u64) -> u64 {
        if hi <= lo {
            lo
        } else {
            match (hi - lo).checked_add(1) {
                Some(width) => lo + self.below(width),
                None => self.next_u64(),
            }
        }
    }

    /// True with a chance of `per_mille` in a thousand.
    pub fn chance(&mut self, per_mille: u32) -> bool {
        self.below(1000) < u64::from(per_mille)
    }
}
