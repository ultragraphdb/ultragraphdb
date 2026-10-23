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
//! A vector predicate `<pred>` keeps its index in three predicates: every node's neighbour lists, all levels in one
//! little-endian matrix, at the node's uid under `<pred>__vector_`; the entry point's uid, big-endian, at uid 1 of
//! `<pred>__vector_entry`; and the deleted nodes, a JSON array, at uid 1 of `<pred>__vector_dead`, which index
//! maintenance writes with [`add_dead_node`].
//!
//! The distance kernels compute as Go's do on amd64, unfused, so their results equal Dgraph's bit for bit.
//!
//! ```
//! use ultragraphdb_store_hnsw::{HnswFloat, decode_uint64_matrix, encode_uint64_matrix, parse_edges};
//!
//! let lists = vec![vec![7, 9], vec![], vec![3]];
//! assert_eq!(decode_uint64_matrix(&encode_uint64_matrix(&lists)), Ok(lists));
//! assert_eq!(parse_edges("[3,5]").unwrap(), vec![3, 5]);
//! assert_eq!(f32::euclidean(&[0.0, 3.0], &[4.0, 0.0]), 5.0);
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
pub use crate::types::vi_txn::ViTxn;
pub use crate::utils::distance::{cosine_similarity, dot_product, euclidean_distance};
pub use crate::utils::helper::{
    NOT_A_UID, add_dead_node, bytes_as_float_array, bytes_to_uint64, concat_strings, insert_layer,
    insert_uids_top_k_by_score, is_equal, parse_edges, uint64_to_bytes, worst_score,
};
pub use crate::utils::matrix::{decode_uint64_matrix, encode_uint64_matrix};
