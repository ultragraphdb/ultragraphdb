/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_sim::{Seed, Stream};

#[test]
fn test_a_seed_derives_the_same_stream_every_time() {
    let mut a = Seed::new(7).stream(Stream::Workload);
    let mut b = Seed::new(7).stream(Stream::Workload);
    assert_eq!((a.next_u64(), a.next_u64()), (b.next_u64(), b.next_u64()));
}

#[test]
fn test_the_streams_of_one_seed_differ() {
    let streams = [
        Stream::Workload,
        Stream::Faults,
        Stream::Process,
        Stream::Scheduler,
        Stream::Cache,
        Stream::Executor,
    ];
    let mut firsts: Vec<u64> = streams
        .iter()
        .map(|s| Seed::new(7).stream(*s).next_u64())
        .collect();
    firsts.sort_unstable();
    firsts.dedup();
    assert_eq!(firsts.len(), streams.len());
    assert_ne!(
        Seed::new(7).stream(Stream::Faults),
        Seed::new(8).stream(Stream::Faults)
    );
}

#[test]
fn test_a_seed_prints_as_its_number() {
    assert_eq!(Seed::new(42).to_string(), "42");
    assert_eq!(Seed::new(42).value(), 42);
}
