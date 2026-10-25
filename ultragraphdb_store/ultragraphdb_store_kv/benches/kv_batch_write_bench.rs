/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Measures `Store::apply` on `FjallStore`: one write batch of 1, 64 or 1,024 fresh keys with values of 64 B or
//! 4 KiB, under each `Durability` mode (`process` hands the batch to the operating system, `filesystem` syncs it),
//! with throughput in value bytes. The batch is built outside the timed routine. It does not measure the posting
//! lists above the store, the writer lock under contention, the periodic sync of `process`, or the stalls a store
//! larger than a run reaches.

// Each bench uses part of what the benches share.
#[allow(dead_code)]
mod bench_dir;

use std::cell::Cell;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use ultragraphdb_store_kv::Durability;
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

use crate::bench_dir::{BenchStore, key};

const BATCH_SIZES: [u64; 3] = [1, 64, 1_024];
const VALUE_SIZES: [usize; 2] = [64, 4_096];

fn batch_writes(c: &mut Criterion) {
    for (mode, durability) in [
        ("process", Durability::Process),
        ("filesystem", Durability::Filesystem),
    ] {
        let mut group = c.benchmark_group(format!("kv_batch_write/{mode}"));
        if durability == Durability::Filesystem {
            // Every batch waits for a sync: fewer samples keep the group's run short.
            group.sample_size(10);
        }
        let bench = BenchStore::open(&format!("write-{mode}"), durability);
        let next = Cell::new(0u64);
        for &value_len in &VALUE_SIZES {
            for &batch_len in &BATCH_SIZES {
                group.throughput(Throughput::Bytes(batch_len * value_len as u64));
                group.bench_with_input(
                    BenchmarkId::new(format!("{value_len}B"), batch_len),
                    &batch_len,
                    |b, &batch_len| {
                        b.iter_batched(
                            || {
                                let version = next.get() + 1;
                                next.set(version);
                                let mut batch = WriteBatch::new();
                                (0..batch_len).for_each(|i| {
                                    batch.push(Entry::put(
                                        key(b"\x00w", version * 2_048 + i),
                                        version,
                                        0,
                                        vec![7; value_len],
                                    ))
                                });
                                batch.set_applied_ts(version);
                                batch
                            },
                            |batch| {
                                bench
                                    .store()
                                    .apply(batch, PersistMode::Buffer)
                                    .expect("the batch is written")
                            },
                            BatchSize::SmallInput,
                        )
                    },
                );
            }
        }
        group.finish();
    }
}

criterion_group!(benches, batch_writes);
criterion_main!(benches);
