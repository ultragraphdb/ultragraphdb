/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Dgraph's `tok/hnsw`: the HNSW vector index, stored in posting lists. [`PersistentHnsw`] inserts a vector and
//! searches for the nearest neighbours of a vector or of a uid's vector, through the cache types of
//! `ultragraphdb_traits_index`: an insert writes through a [`TxnCache`] over a transaction, a search reads through a
//! [`QueryCache`] over a local cache. [`ViTxn`] is both over a request of the posting crate's local commit, so an
//! insert's neighbour lists are committed in the request's batch. It implements the vector index traits of
//! `ultragraphdb_traits_index`, through which index maintenance calls it, and [`HnswFactory`] makes it.
//!
//! A vector predicate keeps its index in the vector-index keyspace under its graph and predicate: every node's
//! neighbour lists, all levels in one matrix of system IDs encoded little-endian, under the node's ID; the entry
//! point, its node's ID as 16 big-endian bytes, and the deleted nodes, a JSON array of IDs as 32 lowercase hexadecimal
//! digits that index maintenance writes with [`add_dead_node`], both under the predicate's own ID.
//!
//! The distance kernels compute as Go's do on amd64, unfused, so their results equal Dgraph's bit for bit.
//!
//! ```
//! use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
//! use ultragraphdb_store_hnsw::{HnswFloat, decode_id_matrix, encode_id_matrix, parse_edges};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let id = |sequence| -> Result<SystemId, Box<dyn std::error::Error>> {
//!     Ok(SystemId::new(TypePrefix::RESOURCE, Bucket::new(0)?, Sequence::new(sequence)?))
//! };
//! let lists = vec![vec![id(7)?, id(9)?], vec![], vec![id(3)?]];
//! assert_eq!(decode_id_matrix(&encode_id_matrix(&lists)), Ok(lists));
//! assert_eq!(parse_edges(r#"["00010000000000000003000000000000"]"#)?, vec![id(3)?]);
//! assert_eq!(f32::euclidean(&[0.0, 3.0], &[4.0, 0.0]), 5.0);
//! # Ok(())
//! # }
//! ```

mod errors;
mod traits;
mod types;
mod utils;

pub use crate::errors::hnsw_error::{HnswError, HnswErrorEnum};
pub use crate::traits::hnsw_float::HnswFloat;
pub use crate::types::candidate_heap::CandidateHeap;
pub use crate::types::hnsw_factory::HnswFactory;
pub use crate::types::persistent_heap_element::PersistentHeapElement;
pub use crate::types::persistent_hnsw::PersistentHnsw;
pub use crate::types::query_cache::QueryCache;
pub use crate::types::similarity_type::SimilarityType;
pub use crate::types::txn_cache::TxnCache;
pub use crate::types::vector_keys::{DEAD_LIST, ENTRY_LIST, NEIGHBOUR_LIST, VectorKeys};
pub use crate::types::vi_txn::{ViTxn, index_posting};
pub use crate::utils::distance::{cosine_similarity, dot_product, euclidean_distance};
pub use crate::utils::helper::{
    add_dead_node, bytes_as_float_array, bytes_to_id, concat_strings, insert_layer,
    insert_uids_top_k_by_score, is_equal, parse_edges, worst_score,
};
pub use crate::utils::matrix::{decode_id_matrix, encode_id_matrix};
