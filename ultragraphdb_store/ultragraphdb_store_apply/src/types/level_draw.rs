/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The seeded source of the vector index's level draws, uniform in [0, 1): SplitMix64, as the simulator's streams
/// are. Dgraph draws from Go's global random source; the port takes its seed from the caller, so a run replays.
#[derive(Debug, Clone)]
pub struct LevelDraw {
    state: u64,
}

impl LevelDraw {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next draw, a multiple of 2⁻⁵³ in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    }
}
