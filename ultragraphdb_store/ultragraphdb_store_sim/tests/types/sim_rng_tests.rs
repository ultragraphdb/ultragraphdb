/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_sim::SimRng;

#[test]
fn test_splitmix64_matches_its_reference_output() {
    // The first outputs of SplitMix64 seeded with 0, as Vigna's reference implementation gives them.
    let mut rng = SimRng::new(0);
    assert_eq!(rng.next_u64(), 0xE220_A839_7B1D_CDAF);
    assert_eq!(rng.next_u64(), 0x6E78_9E6A_A1B9_65F4);
}

#[test]
fn test_bounded_draws_stay_in_their_bounds() {
    let mut rng = SimRng::new(3);
    assert!((0..1000).all(|_| rng.below(7) < 7));
    assert!((0..1000).all(|_| (5..=9).contains(&rng.between(5, 9))));
    assert!((0..1000).all(|_| rng.index(3) < 3));
}

#[test]
fn test_degenerate_bounds_have_one_answer() {
    let mut rng = SimRng::new(3);
    assert_eq!(rng.below(0), 0);
    assert_eq!(rng.index(0), 0);
    assert_eq!(rng.between(9, 5), 9);
    assert_eq!(rng.between(4, 4), 4);
}

#[test]
fn test_chances_of_none_and_all() {
    let mut rng = SimRng::new(3);
    assert!((0..1000).all(|_| !rng.chance(0)));
    assert!((0..1000).all(|_| rng.chance(1000)));
    let hits = (0..10_000).filter(|_| rng.chance(250)).count();
    assert!((2000..3000).contains(&hits), "{hits}");
}
