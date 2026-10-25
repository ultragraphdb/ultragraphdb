/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Measures `Store::drop_prefixes` on `FjallStore` over a fresh store of 100,000 keys under the dropped prefix and
//! 1,000 outside it: the synced drop record, the removals in batches of 10,000, the synced entries and the record's
//! deletion, through to the call's return. Throughput is in keys dropped. The store is built outside the timed
//! routine. It does not measure the cache's epoch above the store, a drop of everything, or the compactions that
//! later reclaim the removed versions.

// Each bench uses part of what the benches share.
#[allow(dead_code)]
mod bench_dir;

use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use ultragraphdb_store_kv::Durability;
use ultragraphdb_traits_store::Store;

use crate::bench_dir::{BenchStore, fill};

const DROPPED: u64 = 100_000;

fn with_prefix() -> BenchStore {
    let bench = BenchStore::open("drop", Durability::Process);
    fill(bench.store(), b"\x00d", DROPPED, 2, 64);
    fill(bench.store(), b"\x00k", 1_000, 2, 64);
    bench
}

fn prefix_drop(c: &mut Criterion) {
    let mut group = c.benchmark_group("kv_drop_prefix");
    group.sample_size(10);
    group.throughput(Throughput::Elements(DROPPED));
    group.bench_function("100k_keys", |b| {
        b.iter_batched(
            with_prefix,
            |bench| {
                bench
                    .store()
                    .drop_prefixes(9, &[b"\x00d".to_vec()], Vec::new())
                    .expect("the prefix drops");
                bench
            },
            BatchSize::PerIteration,
        )
    });
    group.finish();
}

criterion_group!(benches, prefix_drop);
criterion_main!(benches);
