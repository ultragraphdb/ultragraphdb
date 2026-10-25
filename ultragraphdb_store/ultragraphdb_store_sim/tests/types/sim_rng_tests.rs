/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_sim::SimRng;

#[test]
fn test_splitmix64_matches_its_reference_output() {
    // The first outputs of SplitMix64 as Vigna's reference implementation (splitmix64.c) gives them, seeded with
    // 0, 3 and u64::MAX; the last wraps the state on its first step.
    let cases: [(u64, &[u64]); 3] = [
        (
            0,
            &[
                0xE220_A839_7B1D_CDAF,
                0x6E78_9E6A_A1B9_65F4,
                0x06C4_5D18_8009_454F,
                0xF88B_B8A8_724C_81EC,
            ],
        ),
        (
            3,
            &[
                0x1D0B_14E4_DB01_8FED,
                0xB346_6F8A_7B81_A989,
                0x9CEB_E8A6_D050_DD01,
                0x12A7_64FB_66AB_C9CF,
            ],
        ),
        (u64::MAX, &[0xE4D9_7177_1B65_2C20]),
    ];
    for (seed, expected) in cases {
        let mut rng = SimRng::new(seed);
        let got: Vec<u64> = expected.iter().map(|_| rng.next_u64()).collect();
        assert_eq!(got, expected, "seed {seed}");
    }
}

#[test]
fn test_bounded_draws_reduce_the_next_output() {
    // Seed 3's outputs reduced by hand from the reference values above: below(n) is the output modulo n, index
    // likewise, between(lo, hi) is lo plus the output modulo hi - lo + 1, chance(p) is the output modulo 1000
    // below p.
    let mut rng = SimRng::new(3);
    assert_eq!(rng.below(7), 0x1D0B_14E4_DB01_8FED % 7);
    assert_eq!(rng.index(3), (0xB346_6F8A_7B81_A989u64 % 3) as usize);
    assert_eq!(rng.between(5, 9), 5 + 0x9CEB_E8A6_D050_DD01 % 5);
    assert!(!rng.chance(647), "647 is not below 647");
    let mut rng = SimRng::new(3);
    assert!(rng.chance(54), "53 is below 54");
    assert!(!rng.chance(561), "561 is not below 561");
}

#[test]
fn test_bounded_draws_stay_in_their_bounds() {
    let mut rng = SimRng::new(3);
    assert!((0..1000).all(|_| rng.below(7) < 7));
    assert!((0..1000).all(|_| (5..=9).contains(&rng.between(5, 9))));
    assert!((0..1000).all(|_| rng.index(3) < 3));
    assert!((0..1000).all(|_| rng.below(1) == 0));
    let mut seen = [false; 5];
    (0..1000).for_each(|_| seen[(rng.between(5, 9) - 5) as usize] = true);
    assert_eq!(seen, [true; 5], "both bounds are reachable");
}

#[test]
fn test_degenerate_bounds_have_one_answer_and_draw_nothing() {
    let mut rng = SimRng::new(3);
    assert_eq!(rng.below(0), 0);
    assert_eq!(rng.index(0), 0);
    assert_eq!(rng.between(9, 5), 9);
    assert_eq!(rng.between(4, 4), 4);
    assert_eq!(
        rng.next_u64(),
        0x1D0B_14E4_DB01_8FED,
        "no degenerate draw advanced the stream"
    );
}

#[test]
fn test_the_widest_ranges_take_the_whole_output() {
    // between(0, u64::MAX) spans every u64: its width does not fit a u64, so the draw is the output itself.
    let mut rng = SimRng::new(0);
    assert_eq!(rng.between(0, u64::MAX), 0xE220_A839_7B1D_CDAF);
    assert_eq!(rng.between(1, u64::MAX), 1 + 0x6E78_9E6A_A1B9_65F4);
    assert_eq!(rng.below(u64::MAX), 0x06C4_5D18_8009_454F);
}

#[test]
fn test_chances_of_none_and_all() {
    let mut rng = SimRng::new(3);
    assert!((0..1000).all(|_| !rng.chance(0)));
    assert!((0..1000).all(|_| rng.chance(1000)));
    assert!(
        (0..1000).all(|_| rng.chance(u32::MAX)),
        "above a thousand is certain"
    );
    let hits = (0..10_000).filter(|_| rng.chance(250)).count();
    assert!((2000..3000).contains(&hits), "{hits}");
}

#[test]
fn test_clones_replay_the_same_stream() {
    let mut rng = SimRng::new(11);
    rng.next_u64();
    let mut clone = rng.clone();
    assert_eq!(clone, rng);
    assert_eq!(
        (0..4).map(|_| rng.next_u64()).collect::<Vec<_>>(),
        (0..4).map(|_| clone.next_u64()).collect::<Vec<_>>()
    );
}
