/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Dgraph's `posting` over the [`Store`](ultragraphdb_traits_store::Store) trait: posting lists read by merging
//! their deltas over their complete version, rolled up and split; a request's working set and its scratch layers, and
//! its writes apart from it; the commit of a round of transactions as one store batch at the versions the commit step
//! gives them, and of a batch of requests at the next versions; the read gate at the visibility watermark; the
//! posting cache behind [`ListCache`](ultragraphdb_traits_store::ListCache); the rollup worker on an
//! [`Executor`](ultragraphdb_traits_store::Executor).
//!
//! A request owns its copy of a list: its postings write the list's scratch layer at the request's read timestamp,
//! and a commit adds that layer as a delta at its commit timestamp.
//!
//! ```
//! use ultragraphdb_core_codec::Posting;
//! use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
//! use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey};
//! use ultragraphdb_store_posting::{List, ListOptions, SET};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let id = |sequence| -> Result<SystemId, Box<dyn std::error::Error>> {
//!     Ok(SystemId::new(TypePrefix::RESOURCE, Bucket::new(0)?, Sequence::new(sequence)?))
//! };
//! let key = QuadKey::Forward { namespace: 0, graph: DEFAULT_GRAPH, predicate: id(7)?, subject: id(1)? };
//! let mut list = List::empty(key.to_bytes());
//! list.set_ts(5);
//! // A reference's posting is keyed by its ID's 128 bits.
//! list.add_posting(Posting::default().with_uid(id(2)?.to_u128()).with_op(SET), false);
//! list.commit_mutation(6);
//!
//! assert_eq!(list.uids(&ListOptions::at(6))?.ids(), &[id(2)?]);
//! assert!(list.uids(&ListOptions::at(5))?.ids().is_empty());
//! # Ok(())
//! # }
//! ```

mod errors;
mod traits;
mod types;
mod utils;

pub use crate::errors::posting_error::{PostingError, PostingErrorEnum};
pub use crate::traits::list_source::ListSource;
pub use crate::types::batch::Batch;
pub use crate::types::cache_miss::CacheMiss;
pub use crate::types::cached_list::CachedList;
pub use crate::types::list::List;
pub use crate::types::list::from_backup_posting_list;
pub use crate::types::list_options::ListOptions;
pub use crate::types::lists::Lists;
pub use crate::types::lists_config::ListsConfig;
pub use crate::types::mutable_layer::MutableLayer;
pub use crate::types::posting_identity::PostingIdentity;
pub use crate::types::read_pin::ReadPin;
pub use crate::types::request::Request;
pub use crate::types::rollup_output::{RollupOutput, bin_split, should_split};
pub use crate::types::rollup_queues::{ROLLUP_BATCH, ROLLUP_DEDUP_SECS, ROLLUP_QUEUE};
pub use crate::types::timestamps::Timestamps;
pub use crate::types::walk_bounds::WalkBounds;
pub use crate::types::walk_end::WalkEnd;
pub use crate::types::walk_step::WalkStep;
pub use crate::types::writes::Writes;
pub use crate::utils::constants::{
    BIT_COMPLETE_POSTING, BIT_DELTA_POSTING, BIT_EMPTY_POSTING, BIT_SCHEMA_POSTING, BLOCK_SIZE,
    DEL, HIGH_PRIORITY_DELTAS, MAX_LIST_SIZE, OVR, SET,
};
pub use crate::utils::list_keys::{is_quad_list, is_split_key, split_key_of};
pub use crate::utils::posting_codec::{marshal, unmarshal_complete, unmarshal_delta};
pub use crate::utils::postings::{
    delete_all_posting, has_delete_all, is_plist_empty, length_delta,
};
pub use crate::utils::walk_lists::walk_lists;
