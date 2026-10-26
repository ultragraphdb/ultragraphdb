/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The quad layout's keys: [`QuadKey`], one variant per [`Keyspace`], each key the keyspace byte, the namespace as a
//! u64 big-endian, then the variant's fields; the split form of a list key, which sets [`SPLIT_FLAG`] in the first
//! byte and appends the part's start uid; and the [`Region`] a key falls in.
//!
//! ```
//! use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey, ROOT_NAMESPACE};
//!
//! let key = QuadKey::Forward {
//!     namespace: ROOT_NAMESPACE,
//!     graph: DEFAULT_GRAPH,
//!     predicate: 7,
//!     subject: 42,
//! };
//! let bytes = key.to_bytes();
//! assert_eq!(QuadKey::parse(&bytes).unwrap(), (key, None));
//! ```

mod errors;
mod types;

pub use crate::errors::quad_key_error::{QuadKeyError, QuadKeyErrorEnum};
pub use crate::types::count_direction::CountDirection;
pub use crate::types::keyspace::{Keyspace, SPLIT_FLAG};
pub use crate::types::quad_key::{DEFAULT_GRAPH, QuadKey, ROOT_NAMESPACE, key_prefix};
pub use crate::types::region::Region;
