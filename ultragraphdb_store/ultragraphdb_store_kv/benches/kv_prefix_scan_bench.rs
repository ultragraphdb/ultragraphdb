/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Measures a prefix scan of a `FjallSnapshot` read to its end, forward and in reverse, over prefixes of 1,000 and
//! 100,000 keys of one version each, compacted into the last level first. Throughput is in keys scanned; the
//! snapshot is opened inside the timed routine. It does not measure scans of all versions, scans that stop early,
//! the posting-list decoding above the store, or scans over versions the compaction filter has yet to collect.

// Each bench uses part of what the benches share.
#[allow(dead_code)]
mod bench_dir;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use ultragraphdb_store_kv::Durability;
use ultragraphdb_traits_store::{Scan, Store, StoreSnapshot};

use crate::bench_dir::{BenchStore, fill};

fn prefix_scans(c: &mut Criterion) {
    let bench = BenchStore::open("scan", Durability::Process);
    let store = bench.store();
    fill(store, b"\x00a", 1_000, 2, 64);
    fill(store, b"\x00b", 100_000, 2, 64);
    store.compact().expect("the store compacts");

    let mut group = c.benchmark_group("kv_prefix_scan");
    for (prefix, keys) in [(b"\x00a", 1_000u64), (b"\x00b", 100_000)] {
        if keys > 1_000 {
            group.sample_size(20);
        }
        group.throughput(Throughput::Elements(keys));
        for (direction, scan) in [
            ("forward", Scan::prefix(prefix.to_vec())),
            ("reverse", Scan::prefix(prefix.to_vec()).reverse()),
        ] {
            group.bench_with_input(BenchmarkId::new(direction, keys), &scan, |b, scan| {
                b.iter(|| {
                    let snapshot = store.snapshot(10).expect("a snapshot");
                    let scanned = snapshot.scan(scan).filter(Result::is_ok).count();
                    assert_eq!(scanned as u64, keys);
                    scanned
                })
            });
        }
    }
    group.finish();
}

criterion_group!(benches, prefix_scans);
criterion_main!(benches);
