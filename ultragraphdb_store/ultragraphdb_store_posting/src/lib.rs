/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Dgraph's `posting` over the [`Store`](ultragraphdb_traits_store::Store) trait: posting lists read by merging
//! their deltas over their complete version, rolled up and split; a request's working set and its scratch layers;
//! the commit of a batch of requests as one store batch at commit timestamps the single writer hands out; the read
//! gate; the posting cache behind [`ListCache`](ultragraphdb_traits_store::ListCache); the rollup worker on an
//! [`Executor`](ultragraphdb_traits_store::Executor).
//!
//! A request owns its copy of a list: its edges write the list's scratch layer at the request's read timestamp,
//! and a commit adds that layer as a delta at its commit timestamp.
//!
//! ```
//! use ultragraphdb_core_keys::{attr_in_root_namespace, data_key};
//! use ultragraphdb_proto_pb::pb::{DirectedEdge, directed_edge};
//! use ultragraphdb_store_posting::{List, ListOptions};
//!
//! let key = data_key(&attr_in_root_namespace("friend"), 1).unwrap();
//! let mut list = List::empty(key);
//! list.set_ts(5);
//! let edge = DirectedEdge { value_id: 2, op: directed_edge::Op::Set as i32, ..DirectedEdge::default() };
//! list.add_mutation(&edge, None).unwrap();
//! list.commit_mutation(6);
//!
//! assert_eq!(list.uids(&ListOptions::at(6)).unwrap().uids, vec![2]);
//! assert!(list.uids(&ListOptions::at(5)).unwrap().uids.is_empty());
//! ```

mod errors;
mod types;
mod utils;

pub use crate::errors::posting_error::{PostingError, PostingErrorEnum};
pub use crate::types::batch::Batch;
pub use crate::types::cache_miss::CacheMiss;
pub use crate::types::cached_list::CachedList;
pub use crate::types::list::List;
pub use crate::types::list::from_backup_posting_list;
pub use crate::types::list_options::ListOptions;
pub use crate::types::lists::Lists;
pub use crate::types::lists_config::ListsConfig;
pub use crate::types::mutable_layer::MutableLayer;
pub use crate::types::request::Request;
pub use crate::types::rollup_output::{RollupOutput, bin_split, should_split};
pub use crate::types::rollup_queues::{ROLLUP_BATCH, ROLLUP_DEDUP_SECS, ROLLUP_QUEUE};
pub use crate::types::timestamps::Timestamps;
pub use crate::utils::constants::{
    BIT_COMPLETE_POSTING, BIT_DELTA_POSTING, BIT_EMPTY_POSTING, BIT_SCHEMA_POSTING, BLOCK_SIZE,
    DEL, HIGH_PRIORITY_DELTAS, MAX_LIST_SIZE, OVR, SET,
};
pub use crate::utils::posting_codec::{marshal, unmarshal_complete, unmarshal_delta};
pub use crate::utils::postings::{
    delete_all_posting, fingerprint_edge, has_delete_all, is_plist_empty, length_delta, new_posting,
};
