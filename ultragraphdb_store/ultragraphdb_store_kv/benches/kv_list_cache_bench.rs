/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Measures `MokaListCache` through the `ListCache` trait, holding `Arc`s of 256-byte values under a 64 MiB weight
//! bound: a hit among 10,000 entries, a miss, and an insert of a new key, which reaches moka's eviction once the
//! bound is full. Throughput is in operations. It does not measure the posting cache's validity rule or its epoch,
//! contention between threads, or the lists' decoding.

use std::cell::Cell;
use std::sync::Arc;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use ultragraphdb_store_kv::MokaListCache;
use ultragraphdb_traits_store::ListCache;

const ENTRIES: u64 = 10_000;

fn key(i: u64) -> Vec<u8> {
    let mut key = b"\x00c".to_vec();
    key.extend_from_slice(&i.to_be_bytes());
    key
}

fn list_cache(c: &mut Criterion) {
    let cache: MokaListCache<Arc<Vec<u8>>> =
        MokaListCache::new(64 << 20, |k: &[u8], v: &Arc<Vec<u8>>| {
            u32::try_from(k.len() + v.len()).unwrap_or(u32::MAX)
        });
    let value = Arc::new(vec![7u8; 256]);
    (0..ENTRIES).for_each(|i| cache.insert(key(i), value.clone()));
    cache.run_pending_tasks();

    let mut group = c.benchmark_group("kv_list_cache");
    group.throughput(Throughput::Elements(1));
    let next = Cell::new(0u64);
    group.bench_function("hit", |b| {
        b.iter(|| {
            let i = next.get();
            next.set((i + 7_919) % ENTRIES);
            cache.get(&key(i)).expect("a hit")
        })
    });
    group.bench_function("miss", |b| {
        b.iter(|| {
            let i = next.get();
            next.set((i + 7_919) % ENTRIES);
            cache.get(&key(ENTRIES + i))
        })
    });
    let fresh = Cell::new(2 * ENTRIES);
    group.bench_function("insert", |b| {
        b.iter(|| {
            let i = fresh.get();
            fresh.set(i + 1);
            cache.insert(key(i), value.clone())
        })
    });
    group.finish();
}

criterion_group!(benches, list_cache);
criterion_main!(benches);
