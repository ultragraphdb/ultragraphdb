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

mod types;
mod utils;

pub use crate::types::durability::Durability;
pub use crate::types::fjall_scan::FjallScan;
pub use crate::types::fjall_snapshot::FjallSnapshot;
pub use crate::types::fjall_store::{
    DATA_KEYSPACE, FjallStore, META_KEYSPACE, TEMPORARY_KEYSPACE_PREFIX,
};
pub use crate::types::gc_filter::GcFilter;
pub use crate::types::moka_list_cache::MokaListCache;
pub use crate::types::thread_executor::ThreadExecutor;
