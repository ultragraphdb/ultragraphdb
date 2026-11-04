/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What both runs open: a changeset applier over the timed fjall store with moka's cache, its list reads counted, its
//! catalog the Dgraph runs' schema as declarations.

use std::num::NonZeroU64;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use ultragraphdb_core_mapping::RandomSource;
use ultragraphdb_store_apply::{Applied, ChangesetApplier};
use ultragraphdb_store_kv::{Durability, FjallStore, MokaListCache, ThreadExecutor};
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_traits_blocks::TestBlockSource;

use crate::errors::bench_error::BenchError;
use crate::types::commit_shape::CommitShape;
use crate::types::counting_cache::CountingCache;
use crate::types::dir_sizes::DirSizes;
use crate::types::read_counts::ReadCounts;
use crate::types::size::Size;
use crate::types::summary::Summary;
use crate::types::timed_store::TimedStore;

/// The applier both runs drive.
pub type BenchApplier = ChangesetApplier<
    TimedStore,
    CountingCache<MokaListCache<CachedList>>,
    ThreadExecutor,
    TestBlockSource,
>;

/// Dgraph's share of `--cache size-mb` given to the posting-list cache (`percentage=40,40,20`).
const POSTING_CACHE_PERCENT: u64 = 40;

/// The sequences of each block the applier leases. No metadata group runs beside the bench, so the in-memory source
/// stands in for it.
const BLOCK_LEN: NonZeroU64 = NonZeroU64::MIN.saturating_add((1 << 20) - 1);

/// An applier over a fresh store in `dir`, which must not exist yet, its posting cache 40 % of `cache_mb`. Shared,
/// since the apply run's rollup thread rolls lists up beside its apply thread.
///
/// # Errors
///
/// A directory that exists, and the store's and the applier's errors at open.
pub fn open_applier(
    dir: &Path,
    durability: Durability,
    cache_mb: u64,
) -> Result<Arc<BenchApplier>, BenchError> {
    if dir.exists() {
        return Err(BenchError::usage(format!(
            "{} exists; every run takes a fresh directory",
            dir.display()
        )));
    }
    let reads = Arc::new(ReadCounts::new());
    let store = TimedStore::new(FjallStore::open(dir, durability)?, Arc::clone(&reads));
    let cache = CountingCache::new(
        MokaListCache::new(
            cache_mb * (1 << 20) * POSTING_CACHE_PERCENT / 100,
            |_: &[u8], v: &CachedList| v.weight(),
        ),
        reads,
    );
    let lists = Lists::new(store, cache, ListsConfig::default());
    Ok(Arc::new(ChangesetApplier::new(
        lists,
        TestBlockSource::new(BLOCK_LEN),
        RandomSource::new(1),
        ThreadExecutor::new(),
        1,
    )?))
}

/// Declares the predicates of `types` relation types, waits out any build it started, and writes the hubs.
///
/// # Errors
///
/// The applier's.
pub fn prepare(a: &Arc<BenchApplier>, shape: &CommitShape, types: u64) -> Result<(), BenchError> {
    let _declared: Applied = a.apply(&CommitShape::declarations(types))?;
    // Ends when the build the declarations started ends.
    while a.rebuilding() {
        std::thread::sleep(Duration::from_millis(10));
    }
    let _hubs: Applied = a.apply(&shape.hubs())?;
    Ok(())
}

/// A size as JSON: the bytes allocated, as `du` reports them, and the bytes the files hold.
pub fn size_json(size: Size) -> String {
    format!(
        "{{\"du_bytes\":{},\"apparent_bytes\":{}}}",
        size.allocated(),
        size.apparent()
    )
}

/// A directory's sizes as JSON: its data, journal and metadata files and their total, each as [`size_json`].
pub fn files_json(s: DirSizes) -> String {
    format!(
        "{{\"data\":{},\"journal\":{},\"metadata\":{},\"total\":{}}}",
        size_json(s.data()),
        size_json(s.journal()),
        size_json(s.metadata()),
        size_json(s.total())
    )
}

/// A summary as JSON: its nearest-rank median and 99th percentile, its maximum and its mean to a tenth.
pub fn summary_json(s: Summary) -> String {
    format!(
        "{{\"p50\":{},\"p99\":{},\"max\":{},\"mean\":{:.1}}}",
        s.p50(),
        s.p99(),
        s.max(),
        s.mean()
    )
}

/// An optional number as JSON.
pub fn opt_json<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| "null".to_string(), |v| v.to_string())
}
