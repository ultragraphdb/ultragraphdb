/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The store over fjall: [`FjallStore`], the managed-mode Badger surface Dgraph uses behind
//! `ultragraphdb_traits_store`'s `Store`, with its snapshots and scans; the production `ListCache` over moka,
//! [`MokaListCache`]; and the thread-backed `Executor`, [`ThreadExecutor`]. This is the one storage crate that names
//! fjall, moka, the wall clock and threads; the simulator replaces all three under its seed.
//!
//! ```
//! use ultragraphdb_store_kv::{Durability, FjallStore};
//! use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};
//!
//! let tmp = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, Into::into);
//! let dir = tmp.join(format!("kv-doc-{}", std::process::id()));
//! let store = FjallStore::open(&dir, Durability::Filesystem).expect("opens");
//! let mut batch = WriteBatch::new();
//! batch.push(Entry::put(b"key".to_vec(), 7, 0x04, b"delta".to_vec()));
//! batch.set_applied_ts(7);
//! store.apply(batch, PersistMode::Buffer).expect("applies");
//! assert_eq!(store.get_at(b"key", 9).expect("reads").map(|i| i.version()), Some(7));
//! assert_eq!(store.get_at(b"key", 6).expect("reads"), None);
//! drop(store);
//! let _ = std::fs::remove_dir_all(&dir);
//! ```
//!
//! # Benchmarks
//!
//! Criterion benches under `benches/` measure the store alone, through the `Store` trait on [`FjallStore`] and
//! through [`MokaListCache`]: batch writes by batch and value size under each [`Durability`], point reads, prefix
//! scans, `compact` with the GC filter, a prefix drop, and the list cache. Each file states what it measures and what
//! it does not, and writes its store under the system's temporary directory. They are never part of `make test`.
//!
//! ```text
//! cargo bench -p ultragraphdb_store_kv                          # every bench
//! cargo bench -p ultragraphdb_store_kv --bench kv_compact_bench # one bench
//! bazel run -c opt //ultragraphdb_store/ultragraphdb_store_kv:kv_compact_bench -- --bench
//! ```
//!
//! Under Bazel, `--bench` selects measurement; without it criterion runs each bench once as a check. Criterion writes
//! its reports to `$CRITERION_HOME`, or to `target/criterion` under the working directory.

mod types;
mod utils;

pub use crate::types::drop_stage::DropStage;
pub use crate::types::durability::Durability;
pub use crate::types::fjall_scan::FjallScan;
pub use crate::types::fjall_snapshot::FjallSnapshot;
pub use crate::types::fjall_store::{
    DATA_KEYSPACE, FjallStore, META_KEYSPACE, TEMPORARY_KEYSPACE_PREFIX, TERMS_KEYSPACE,
};
pub use crate::types::gc_filter::GcFilter;
pub use crate::types::moka_list_cache::MokaListCache;
pub use crate::types::thread_executor::ThreadExecutor;
