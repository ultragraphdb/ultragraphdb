/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_mapping::RandomSource;

#[test]
fn test_the_draws_are_splitmix64s() {
    // SplitMix64 seeded 0: 0xe220a8397b1dcdaf, 0x6e789e6aa1b965f4, 0x06c45d188009454f, the reference splitmix64.c's
    // outputs, recomputed in Python from the published algorithm.
    let mut zero = RandomSource::new(0);
    assert_eq!(zero.next_u64(), 0xe220_a839_7b1d_cdaf);
    assert_eq!(zero.next_u64(), 0x6e78_9e6a_a1b9_65f4);
    assert_eq!(zero.next_u64(), 0x06c4_5d18_8009_454f);
    let mut seeded = RandomSource::new(42);
    assert_eq!(seeded.next_u64(), 0xbdd7_3226_2feb_6e95);
}

#[test]
fn test_104_bits_are_two_draws_below_2_to_the_104() {
    // Seeded 42: 0xbdd732262feb6e95's low 40 bits, 0x262feb6e95, above 0x28efe333b266f103.
    let mut seeded = RandomSource::new(42);
    assert_eq!(seeded.next_bits_104(), 0x26_2feb_6e95_28ef_e333_b266_f103);
    let mut source = RandomSource::new(7);
    assert!((0..1_000).all(|_| source.next_bits_104() < 1u128 << 104));
}

#[test]
fn test_a_draw_below_a_bound_stays_below_it() {
    let mut source = RandomSource::new(9);
    for bound in [1u64, 2, 3, 21_846, 65_536, u64::MAX] {
        assert!((0..200).all(|_| source.below(bound) < bound), "{bound}");
    }
    assert_eq!(source.below(1), 0);
    assert_eq!(source.below(0), 0);
}

#[test]
fn test_a_draw_below_a_bound_is_lemires_from_the_seeds_draws() {
    // Lemire's multiply-and-reject over SplitMix64 seeded 42, computed in Python (`python3 -I`) from the published
    // algorithms: a draw is accepted when the low 64 bits of draw × bound are at least 2^64 mod bound, and the value is
    // the high 64 bits. Below 3 no draw is rejected; below 2^63 + 1, where 2^64 mod bound is 2^63 - 1, the first value
    // takes two draws and the second four.
    let mut small = RandomSource::new(42);
    let below_3: Vec<u64> = (0..6).map(|_| small.below(3)).collect();
    assert_eq!(below_3, [2, 0, 0, 1, 0, 2]);
    let mut wide = RandomSource::new(42);
    let below_wide: Vec<u64> = (0..4).map(|_| wide.below((1 << 63) + 1)).collect();
    assert_eq!(
        below_wide,
        [
            1_474_913_046_063_446_145,
            8_007_990_562_831_494_531,
            2_014_432_356_388_812_462,
            7_384_525_663_493_887_954
        ]
    );
}

#[test]
fn test_draws_below_a_bound_reach_every_value() {
    // 3,000 draws below 3 miss a value with probability 3 × (2/3)^3000, which is 0 in practice.
    let mut source = RandomSource::new(11);
    let mut seen = [false; 3];
    for _ in 0..3_000 {
        if let Some(slot) = seen.get_mut(source.below(3) as usize) {
            *slot = true;
        }
    }
    assert_eq!(seen, [true; 3]);
}
