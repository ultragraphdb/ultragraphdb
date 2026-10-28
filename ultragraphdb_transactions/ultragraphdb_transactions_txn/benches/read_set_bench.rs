/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Measures the design choice behind `ReadSet`: a transaction's read keys gathered and sorted with duplicates dropped,
//! as its commit does, in one reused byte arena with u32 spans, against an owned `Vec<u8>` per key sorted in a fresh
//! vector and against a fresh `BTreeSet<Vec<u8>>`. Keys are forward-list keys of 25 bytes, 64 or 1,024 per
//! transaction, one in four a duplicate, in the order a transaction reads them. It does not measure the execution
//! that records them or the resolver that walks them.

use std::collections::BTreeSet;
use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use ultragraphdb_transactions_txn::ReadSet;

/// `n` forward-list keys of 25 bytes in read order, every fourth repeating an earlier one.
fn keys(n: usize) -> Vec<Vec<u8>> {
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    (0..n)
        .map(|i| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let subject = if i % 4 == 3 {
                (i as u64) / 2
            } else {
                state >> 16
            };
            let mut key = vec![0x10];
            key.extend_from_slice(&0u64.to_be_bytes());
            key.extend_from_slice(&0u64.to_be_bytes());
            key.extend_from_slice(&subject.to_be_bytes());
            key
        })
        .collect()
}

fn read_sets(c: &mut Criterion) {
    let mut group = c.benchmark_group("read_set_gather_and_seal");
    for n in [64usize, 1_024] {
        let input = keys(n);
        let mut arena = ReadSet::new();
        group.bench_with_input(BenchmarkId::new("arena_reused", n), &input, |b, input| {
            b.iter(|| {
                arena.reset(0, 1, usize::MAX);
                for key in input {
                    arena.push_list(key).expect("within the limit");
                }
                arena.seal();
                black_box(arena.lists().count())
            })
        });
        group.bench_with_input(BenchmarkId::new("vec_per_key", n), &input, |b, input| {
            b.iter(|| {
                let mut owned: Vec<Vec<u8>> = input.iter().map(|k| k.to_vec()).collect();
                owned.sort_unstable();
                owned.dedup();
                black_box(owned.len())
            })
        });
        group.bench_with_input(BenchmarkId::new("btree_set", n), &input, |b, input| {
            b.iter(|| {
                let set: BTreeSet<Vec<u8>> = input.iter().map(|k| k.to_vec()).collect();
                black_box(set.len())
            })
        });
    }
    group.finish();
}

criterion_group!(benches, read_sets);
criterion_main!(benches);
