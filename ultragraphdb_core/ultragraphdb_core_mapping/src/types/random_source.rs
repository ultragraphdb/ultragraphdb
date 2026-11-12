/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The most draws [`RandomSource::below`] makes before it reduces one: each is rejected with probability below one
/// half.
const BELOW_DRAWS_MAX: usize = 128;

/// The random source a partition injects where it mints names: SplitMix64 (Steele, Lea and Flood, "Fast Splittable
/// Pseudorandom Number Generators", OOPSLA 2014), seeded by its owner, so the simulator replays a run from its seed and
/// a partition seeds it from the operating system at start. It draws a new blank node's bucket among those its
/// partition owns and the 104 random bits of its skolem IRI, and those of a statement's reifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RandomSource {
    state: u64,
}

impl RandomSource {
    /// The source seeded with `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next 64 bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// The next 104 bits, uniform below 2^104: two draws, the first's low 40 bits above the second's 64.
    pub fn next_bits_104(&mut self) -> u128 {
        let high = u128::from(self.next_u64() & ((1 << 40) - 1));
        let low = u128::from(self.next_u64());
        let bits = (high << 64) | low;
        debug_assert!(bits < 1 << 104, "104 bits");
        bits
    }

    /// A draw uniform below `bound`, which is at least 1, by Lemire's multiply-and-reject ("Fast Random Integer
    /// Generation in an Interval", ACM TOMACS 2019); 0 for a bound of 0.
    pub fn below(&mut self, bound: u64) -> u64 {
        if bound == 0 {
            return 0;
        }
        // The draws whose low product falls below 2^64 mod bound are rejected, leaving each value equally likely;
        // fewer than half of all draws are rejected, so a turn ends with probability above one half.
        let threshold = bound.wrapping_neg() % bound;
        for _ in 0..BELOW_DRAWS_MAX {
            let draw = self.next_u64();
            let product = u128::from(draw) * u128::from(bound);
            // The wrapping product is the full product's low 64 bits, the part Lemire's method reads; the cast takes
            // the high 64 bits, which the shift leaves below 2^64, so it loses nothing.
            if draw.wrapping_mul(bound) >= threshold {
                let value = (product >> 64) as u64;
                debug_assert!(value < bound, "a draw lies below its bound");
                return value;
            }
        }
        // Reached with probability below 2^-128: the last draw, reduced, whatever its small bias.
        self.next_u64() % bound
    }
}
