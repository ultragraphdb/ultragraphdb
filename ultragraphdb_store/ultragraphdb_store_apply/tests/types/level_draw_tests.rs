/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_apply::LevelDraw;

/// 2^53, the number of multiples of 2^-53 in [0, 1).
const TWO_53: f64 = 9_007_199_254_740_992.0;

#[test]
fn test_the_draws_are_splitmix64_s_outputs_scaled_to_the_unit_interval() {
    // SplitMix64 seeded with 0 (Vigna, splitmix64.c) outputs 0xE220A8397B1DCDAF, 0x6E789E6AA1B965F4 and
    // 0x06C45D188009454F; each draw is its top 53 bits over 2^53.
    let mut draw = LevelDraw::new(0);
    for output in [
        0xE220_A839_7B1D_CDAFu64,
        0x6E78_9E6A_A1B9_65F4,
        0x06C4_5D18_8009_454F,
    ] {
        assert_eq!(draw.next_f64(), (output >> 11) as f64 / TWO_53);
    }
}

#[test]
fn test_a_seed_replays_and_another_seed_differs() {
    let take = |seed| {
        let mut d = LevelDraw::new(seed);
        (0..64).map(|_| d.next_f64()).collect::<Vec<_>>()
    };
    assert_eq!(take(7), take(7));
    assert_ne!(take(7), take(8));
    // The largest seed wraps its state without overflow.
    for v in take(u64::MAX).into_iter().chain(take(7)) {
        assert!((0.0..1.0).contains(&v), "{v}");
    }
}
