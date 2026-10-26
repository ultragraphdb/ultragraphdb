/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What both runs open: a changeset applier over the timed fjall store with moka's cache, its list reads counted, its
//! catalog the Dgraph runs' schema as declarations.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use ultragraphdb_store_apply::ChangesetApplier;
use ultragraphdb_store_kv::{Durability, FjallStore, MokaListCache, ThreadExecutor};
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};

use crate::errors::bench_error::BenchError;
use crate::types::commit_shape::CommitShape;
use crate::types::counting_cache::CountingCache;
use crate::types::read_counts::ReadCounts;
use crate::types::size::Size;
use crate::types::timed_store::TimedStore;

/// The applier both runs drive.
pub type BenchApplier =
    ChangesetApplier<TimedStore, CountingCache<MokaListCache<CachedList>>, ThreadExecutor>;

/// Dgraph's share of `--cache size-mb` given to the posting-list cache (`percentage=40,40,20`).
const POSTING_CACHE_PERCENT: u64 = 40;

/// An applier over a fresh store in `dir`, which must not exist yet, its posting cache 40 % of `cache_mb`.
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
        return Err(BenchError::Usage(format!(
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
    a.apply(&CommitShape::declarations(types))?;
    while a.rebuilding() {
        std::thread::sleep(Duration::from_millis(10));
    }
    a.apply(&shape.hubs())?;
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

/// An optional number as JSON.
pub fn opt_json<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| "null".to_string(), |v| v.to_string())
}
