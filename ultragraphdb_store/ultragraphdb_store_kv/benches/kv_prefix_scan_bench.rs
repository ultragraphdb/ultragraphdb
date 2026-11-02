/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Measures a prefix scan of a `FjallSnapshot` read to its end, forward and in reverse, over prefixes of 1,000 and
//! 100,000 keys of one version each, compacted into the last level first. Throughput is in keys scanned; the
//! snapshot is opened inside the timed routine. It does not measure scans of all versions, scans that stop early,
//! the posting-list decoding above the store, or scans over versions the compaction filter has yet to collect.

/// What the benches share; each bench uses a part of it.
pub mod bench_dir;

use criterion::{
    BenchmarkGroup, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main,
};
use ultragraphdb_store_kv::Durability;
use ultragraphdb_traits_store::{Scan, Store, StoreSnapshot};

use crate::bench_dir::{BenchStore, fill, run};

fn prefix_scans(c: &mut Criterion) {
    run("kv_prefix_scan", || {
        let bench = BenchStore::open("scan", Durability::Process)?;
        let store = bench.store();
        fill(store, b"\x00a", 1_000, 2, 64)?;
        fill(store, b"\x00b", 100_000, 2, 64)?;
        store.compact()?;

        let mut group = c.benchmark_group("kv_prefix_scan");
        for (prefix, keys) in [(b"\x00a", 1_000u64), (b"\x00b", 100_000)] {
            if keys > 1_000 {
                let _sampled: &mut BenchmarkGroup<'_, _> = group.sample_size(20);
            }
            let _counted: &mut BenchmarkGroup<'_, _> = group.throughput(Throughput::Elements(keys));
            for (direction, scan) in [
                ("forward", Scan::prefix(prefix.to_vec())),
                ("reverse", Scan::prefix(prefix.to_vec()).reverse()),
            ] {
                let _measured: &mut BenchmarkGroup<'_, _> =
                    group.bench_with_input(BenchmarkId::new(direction, keys), &scan, |b, scan| {
                        b.iter(|| {
                            let scanned = store.snapshot(10).map(|snapshot| {
                                snapshot.scan(scan).filter(Result::is_ok).count() as u64
                            });
                            assert_eq!(scanned, Ok(keys));
                            scanned
                        })
                    });
            }
        }
        group.finish();
        Ok(())
    });
}

criterion_group!(benches, prefix_scans);
criterion_main!(benches);
