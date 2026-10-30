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

#[test]
fn test_a_node_s_draws_are_splitmix64_seeded_by_the_hash_of_seed_namespace_predicate_and_uid() {
    // The oracle, scratch levels.py, a separate implementation of splitmix64.c: the state is SplitMix64's output on
    // the seed, then on that output XORed with the namespace, the predicate and the UID in turn; each draw is a later
    // output's top 53 bits over 2^53. (0, 0, 0, 0) hashes to 0x2130748AAAC80268, (1, 0, 5, 7) to 0x0C7AD5836EA83279.
    for ((seed, namespace, predicate, uid), tops) in [
        (
            (0, 0, 0, 0),
            [
                0x000F_15CB_534D_6BFAu64,
                0x0017_312B_456D_0B25,
                0x0009_B26D_D9D0_3FC9,
            ],
        ),
        (
            (1, 0, 5, 7),
            [
                0x0010_2CC2_A630_AC85,
                0x0017_4ACF_7F1A_4CD6,
                0x000E_AF3D_1AF3_8AFB,
            ],
        ),
    ] {
        let mut draw = LevelDraw::of_node(seed, namespace, predicate, uid);
        for top in tops {
            assert_eq!(
                draw.next_f64(),
                top as f64 / TWO_53,
                "{seed} {namespace} {predicate} {uid}"
            );
        }
    }
}

#[test]
fn test_a_node_s_draws_depend_on_each_of_the_four_and_on_nothing_else() {
    let first = |seed, namespace, predicate, uid| {
        let mut d = LevelDraw::of_node(seed, namespace, predicate, uid);
        (d.next_f64(), d.next_f64())
    };
    let base = first(3, 1, 5, 7);
    // The same node draws the same, however many other nodes drew before it.
    let _ = (0..10).map(|u| first(3, 1, 5, u)).collect::<Vec<_>>();
    assert_eq!(first(3, 1, 5, 7), base);
    for other in [
        first(4, 1, 5, 7),
        first(3, 2, 5, 7),
        first(3, 1, 6, 7),
        first(3, 1, 5, 8),
    ] {
        assert_ne!(other, base);
    }
    // Swapping two of the four is another node.
    assert_ne!(first(3, 5, 1, 7), base);
    assert_ne!(first(3, 1, 7, 5), base);
    for v in [
        base.0,
        base.1,
        first(u64::MAX, u64::MAX, u64::MAX, u64::MAX).0,
    ] {
        assert!((0.0..1.0).contains(&v), "{v}");
    }
}
