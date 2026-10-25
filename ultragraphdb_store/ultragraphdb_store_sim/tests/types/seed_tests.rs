/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_sim::{Seed, Stream};

const STREAMS: [Stream; 6] = [
    Stream::Workload,
    Stream::Faults,
    Stream::Process,
    Stream::Scheduler,
    Stream::Cache,
    Stream::Executor,
];

#[test]
fn test_a_seed_derives_the_same_stream_every_time() {
    let mut a = Seed::new(7).stream(Stream::Workload);
    let mut b = Seed::new(7).stream(Stream::Workload);
    assert_eq!((a.next_u64(), a.next_u64()), (b.next_u64(), b.next_u64()));
}

#[test]
fn test_the_streams_of_seed_7_are_pinned() {
    // The first output of each stream of seed 7: SplitMix64 seeded with 7 ^ (stream + 1) * 0xA0761D6478BD642F,
    // computed with Vigna's reference splitmix64.c. A change here changes what every recorded seed replays.
    let expected = [
        0x5AEA_0893_D43C_1309,
        0xD5BD_300D_7485_BFAA,
        0xDE1A_2F50_3644_E647,
        0xCD4E_BD3D_6E9A_094B,
        0xB6A1_54D4_8CCF_325B,
        0xF173_6137_5410_DF47,
    ];
    let firsts: Vec<u64> = STREAMS
        .iter()
        .map(|s| Seed::new(7).stream(*s).next_u64())
        .collect();
    assert_eq!(firsts, expected);
}

#[test]
fn test_the_streams_of_one_seed_differ() {
    let mut firsts: Vec<u64> = STREAMS
        .iter()
        .map(|s| Seed::new(7).stream(*s).next_u64())
        .collect();
    firsts.sort_unstable();
    firsts.dedup();
    assert_eq!(firsts.len(), STREAMS.len());
    assert_ne!(
        Seed::new(7).stream(Stream::Faults),
        Seed::new(8).stream(Stream::Faults)
    );
}

#[test]
fn test_a_seed_prints_as_its_number() {
    for value in [0, 42, u64::MAX] {
        assert_eq!(Seed::new(value).to_string(), value.to_string());
        assert_eq!(Seed::new(value).value(), value);
    }
    assert_eq!(Seed::new(u64::MAX).to_string(), "18446744073709551615");
}
