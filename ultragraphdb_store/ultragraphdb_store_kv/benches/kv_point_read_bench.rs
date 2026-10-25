/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Measures `Store::get_at` on `FjallStore`, each read through its own snapshot, over a store of 100,000 keys and one
//! key with 1,000 versions, compacted into the last level first: the newest version of a key, an older version
//! (timestamp 10) of the key with 1,000 versions, and a key the store does not hold. Throughput is in reads. It does
//! not measure the posting-list cache, multi-key reads through one snapshot, a cold block cache, or reads while
//! writes or compactions run.

// Each bench uses part of what the benches share.
#[allow(dead_code)]
mod bench_dir;

use std::cell::Cell;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use ultragraphdb_store_kv::Durability;
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

use crate::bench_dir::{BenchStore, fill, key};

const KEYS: u64 = 100_000;
const VERSIONS: u64 = 1_000;
const READ_TS: u64 = 1_000_000;

fn point_reads(c: &mut Criterion) {
    let bench = BenchStore::open("read", Durability::Process);
    let store = bench.store();
    fill(store, b"\x00r", KEYS, 2, 64);
    let mut batch = WriteBatch::new();
    (1..=VERSIONS).for_each(|v| batch.push(Entry::put(b"\x00hot".to_vec(), v, 0, vec![7; 64])));
    store
        .apply(batch, PersistMode::Buffer)
        .expect("the versions are written");
    store.compact().expect("the store compacts");

    let mut group = c.benchmark_group("kv_point_read");
    group.throughput(Throughput::Elements(1));
    let next = Cell::new(0u64);
    group.bench_function("newest", |b| {
        b.iter(|| {
            let i = next.get();
            next.set((i + 7_919) % KEYS);
            store.get_at(&key(b"\x00r", i), READ_TS).expect("reads")
        })
    });
    group.bench_function("older_of_1000_versions", |b| {
        b.iter(|| store.get_at(b"\x00hot", 10).expect("reads"))
    });
    group.bench_function("missing", |b| {
        b.iter(|| {
            let i = next.get();
            next.set((i + 7_919) % KEYS);
            store.get_at(&key(b"\x00m", i), READ_TS).expect("reads")
        })
    });
    group.finish();
}

criterion_group!(benches, point_reads);
criterion_main!(benches);
