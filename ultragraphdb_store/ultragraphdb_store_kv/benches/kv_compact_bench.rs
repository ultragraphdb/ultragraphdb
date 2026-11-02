/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Measures `Store::compact` on `FjallStore` with the GC filter: a flush of the memtables and a major compaction of a
//! fresh store of 10,000 keys with 10 versions each (64 B values), version 8 a complete list with the discard flag
//! and the discard timestamp at 9, so the filter collects versions 1 to 7 of every key and the journaled removals of
//! what it collected are written. Throughput is in versions compacted. The store is built outside the timed routine.
//! It does not measure fjall's background compactions, the deletion sweep, or a store larger than memory.

/// What the benches share; each bench uses a part of it.
pub mod bench_dir;

use criterion::{
    BatchSize, BenchmarkGroup, Criterion, Throughput, criterion_group, criterion_main,
};
use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_kv::Durability;
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

use crate::bench_dir::{BenchStore, key};

const KEYS: u64 = 10_000;
const VERSIONS: u64 = 10;
const COMPLETE: u64 = 8;

/// A fresh store holding garbage below its discard timestamp.
fn with_garbage() -> Result<BenchStore, StoreError> {
    let bench = BenchStore::open("compact", Durability::Process)?;
    for version in 1..=VERSIONS {
        let mut batch = WriteBatch::new();
        (0..KEYS).for_each(|i| {
            let entry = Entry::put(key(b"\x00g", i), version, 0, vec![7; 64]);
            batch.push(if version == COMPLETE {
                entry.with_discard()
            } else {
                entry
            });
        });
        bench.store().apply(batch, PersistMode::Buffer)?;
    }
    bench.store().set_discard_ts(COMPLETE + 1)?;
    Ok(bench)
}

fn compaction(c: &mut Criterion) {
    let mut group = c.benchmark_group("kv_compact");
    let _measured: &mut BenchmarkGroup<'_, _> = group
        .sample_size(10)
        .throughput(Throughput::Elements(KEYS * VERSIONS))
        .bench_function("gc_filter_10k_keys_10_versions", |b| {
            b.iter_batched(
                with_garbage,
                |bench| {
                    let compacted = bench.as_ref().map(|bench| bench.store().compact());
                    assert!(
                        matches!(compacted, Ok(Ok(()))),
                        "the store fills and compacts: {compacted:?}"
                    );
                    // Dropped, with its directory, outside the timed routine.
                    bench
                },
                BatchSize::PerIteration,
            )
        });
    group.finish();
}

criterion_group!(benches, compaction);
criterion_main!(benches);
