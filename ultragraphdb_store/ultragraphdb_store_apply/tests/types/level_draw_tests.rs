/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_store_apply::LevelDraw;
use ultragraphdb_tool_bdd::TestResult;

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

/// The system ID of type 1 in `bucket` with `sequence`.
fn id(bucket: u32, sequence: u64) -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::RESOURCE,
        Bucket::new(bucket)?,
        Sequence::new(sequence)?,
    ))
}

#[test]
fn test_a_node_s_draws_are_splitmix64_seeded_by_the_hash_of_seed_namespace_and_both_ids()
-> TestResult {
    // The oracle, a separate implementation of splitmix64.c in Python: the state is SplitMix64's output on the seed,
    // then on that output XORed in turn with the namespace, the predicate's high and low 64 bits and the node's; each
    // draw is a later output's top 53 bits over 2^53. (0, 0, (1, bucket 0, 1), (1, bucket 0, 2)) hashes to
    // 0x840E01843B9DA454, (1, 7, (1, 0xABCDEF, 5), (1, 0xABCDEF, 7)) to 0xB06934141E725F05.
    for ((seed, namespace, predicate, node), tops) in [
        (
            (0, 0, id(0, 1)?, id(0, 2)?),
            [
                0x000B_A37A_1BFC_57D9u64,
                0x001B_F75E_82B2_4E13,
                0x0009_8A71_4468_8DF2,
            ],
        ),
        (
            (1, 7, id(0xAB_CDEF, 5)?, id(0xAB_CDEF, 7)?),
            [
                0x0012_9CF0_497A_0092,
                0x001A_9BCE_393C_8F83,
                0x0012_E0E7_E95B_0D18,
            ],
        ),
    ] {
        let mut draw = LevelDraw::of_node(seed, namespace, predicate, node);
        for top in tops {
            assert_eq!(
                draw.next_f64(),
                top as f64 / TWO_53,
                "{seed} {namespace} {predicate:?} {node:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn test_a_node_s_draws_depend_on_each_of_the_four_and_on_nothing_else() -> TestResult {
    let first = |seed, namespace, predicate, node| {
        let mut d = LevelDraw::of_node(seed, namespace, predicate, node);
        (d.next_f64(), d.next_f64())
    };
    let (p, n) = (id(5, 5)?, id(7, 7)?);
    let base = first(3, 1, p, n);
    // The same node draws the same, however many other nodes drew before it.
    let _others: Vec<(f64, f64)> = (1..10)
        .map(|u| id(7, u).map(|o| first(3, 1, p, o)))
        .collect::<TestResult<_>>()?;
    assert_eq!(first(3, 1, p, n), base);
    for other in [
        first(4, 1, p, n),
        first(3, 2, p, n),
        first(3, 1, id(5, 6)?, n),
        first(3, 1, p, id(7, 8)?),
    ] {
        assert_ne!(other, base);
    }
    // Swapping the predicate and the node is another node; IDs differing in their bucket alone differ too.
    assert_ne!(first(3, 1, n, p), base);
    assert_ne!(first(3, 1, p, id(8, 7)?), base);
    let top = SystemId::from_u128(
        (u128::from(u16::MAX) << 112) | (u128::from(u64::MAX) << 48) | (0xFF_FFFF << 24),
    )?;
    for v in [base.0, base.1, first(u64::MAX, u64::MAX, top, top).0] {
        assert!((0.0..1.0).contains(&v), "{v}");
    }
    Ok(())
}
