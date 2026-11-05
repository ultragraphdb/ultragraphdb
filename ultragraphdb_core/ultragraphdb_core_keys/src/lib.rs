/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The quad layout's keys: [`QuadKey`], one variant per [`Keyspace`], each key the keyspace byte, the tenant as a
//! u64 big-endian, then the variant's fields, every ID a 128-bit system ID as its 16 big-endian bytes and the
//! default graph as 16 zero bytes; a dictionary key's [`DictionaryEntry`], a kind byte and 16 bytes; the split form
//! of a list key, which sets [`SPLIT_FLAG`] in the first byte and appends the part's [`SplitStart`] after its kind
//! byte; and the [`Region`] a key falls in.
//!
//! ```
//! use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
//! use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey, ROOT_TENANT};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let id = |bucket, sequence| -> Result<SystemId, Box<dyn std::error::Error>> {
//!     Ok(SystemId::new(TypePrefix::RESOURCE, Bucket::new(bucket)?, Sequence::new(sequence)?))
//! };
//! let key = QuadKey::Forward {
//!     tenant: ROOT_TENANT,
//!     graph: DEFAULT_GRAPH,
//!     predicate: id(7, 7)?,
//!     subject: id(42, 42)?,
//! };
//! let bytes = key.to_bytes();
//! assert_eq!(QuadKey::parse(&bytes)?, (key, None));
//! # Ok(())
//! # }
//! ```

mod errors;
mod types;

pub use crate::errors::quad_key_error::{QuadKeyError, QuadKeyErrorEnum};
pub use crate::types::count_direction::CountDirection;
pub use crate::types::dictionary_entry::DictionaryEntry;
pub use crate::types::graph::Graph;
pub use crate::types::keyspace::{Keyspace, SPLIT_FLAG};
pub use crate::types::quad_key::{DEFAULT_GRAPH, QuadKey, ROOT_TENANT, key_prefix};
pub use crate::types::region::Region;
pub use crate::types::split_start::SplitStart;
