/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;

/// The seeded source of the vector index's level draws, uniform in [0, 1): SplitMix64, as the simulator's streams
/// are. Dgraph draws from Go's global random source; the port draws a node's levels from a stream of its own, seeded by
/// a hash of the index's seed, the namespace, the predicate and the node ([`LevelDraw::of_node`]), so a node draws the
/// same levels whatever order it is inserted in, and a run replays.
#[derive(Debug, Clone)]
pub struct LevelDraw {
    state: u64,
}

impl LevelDraw {
    /// The draws of the stream seeded with `seed`.
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The draws of `node` of `predicate` in `namespace`, under the index's `seed`: SplitMix64 seeded with the hash of
    /// the seed, the namespace and the two IDs' high and low 64 bits, in that order.
    pub fn of_node(seed: u64, namespace: u64, predicate: SystemId, node: SystemId) -> Self {
        let halves = |id: SystemId| {
            let value = id.to_u128();
            // The high and the low 64 bits: each shift and truncation keeps exactly one half.
            [(value >> 64) as u64, value as u64]
        };
        let [p_high, p_low] = halves(predicate);
        let [n_high, n_low] = halves(node);
        let hash = [namespace, p_high, p_low, n_high, n_low]
            .into_iter()
            .fold(Self::new(seed).next_u64(), |h, x| {
                Self::new(h ^ x).next_u64()
            });
        Self::new(hash)
    }

    /// The next draw, a multiple of 2⁻⁵³ in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// SplitMix64's next output.
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}
