/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The boundary between the port and its storage. Everything above the store reaches it through [`Store`], which
//! `ultragraphdb_store_kv` implements over fjall and `ultragraphdb_store_sim` in memory for the simulator; background
//! work runs on an [`Executor`] the caller passes, and the posting cache sits behind [`ListCache`].
//!
//! A store keeps every version of a key, as Dgraph's managed-mode Badger does: a write names its version, a read
//! names its timestamp and sees the newest version at or below it.
//!
//! ```
//! use ultragraphdb_traits_store::{Entry, Scan, WriteBatch};
//!
//! let mut batch = WriteBatch::new();
//! batch.push(Entry::put(b"key".to_vec(), 7, 0x04, b"delta".to_vec()));
//! batch.push(Entry::deletion(b"gone".to_vec(), 7));
//! batch.set_applied_ts(7);
//! assert_eq!(batch.len(), 2);
//! assert!(batch.check_key_sizes().is_ok());
//!
//! let scan = Scan::prefix(b"ke".to_vec()).reverse().all_versions();
//! assert!(scan.selects(b"key") && !scan.selects(b"kf"));
//! ```

mod traits;
mod types;
mod utils;

pub use crate::traits::executor::Executor;
pub use crate::traits::list_cache::ListCache;
pub use crate::traits::store::Store;
pub use crate::traits::store_snapshot::StoreSnapshot;
pub use crate::types::direction::Direction;
pub use crate::types::entry::Entry;
pub use crate::types::item::Item;
pub use crate::types::persist_mode::PersistMode;
pub use crate::types::rebuild_record::RebuildRecord;
pub use crate::types::scan::Scan;
pub use crate::types::temporary::Temporary;
pub use crate::types::write_batch::WriteBatch;
pub use crate::utils::key_size::{
    MAX_STORED_KEY_SIZE, MAX_USER_KEY_SIZE, check_key_size, stored_key_size,
};
pub use crate::utils::namespace::{NAMESPACE_OFFSET, key_namespace};
