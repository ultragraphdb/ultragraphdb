/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/persistent_hnsw.go (persistentHNSW, applyOptions, fillNeighborEdges,
 * calculateNewEntryVec, PickStartNode), persistent_factory.go (createWithLock) and helper.go
 * (getDataFromKeyWithCacheType, populateEdgeDataFromKeyWithCacheType, getVecFromUid). Changed for ultragraphdb:
 * translated to Rust; the factory's map of indexes is not ported, every insert and search building its index as
 * Dgraph's CreateIndex does; the options are read, not written back, where Dgraph writes the parameters an exponent
 * sets into the caller's options; an option below 1 and a float width other than the vectors' are refused when the
 * index is built, where Dgraph panics at the first search or insert reaching them; a read that fails is an error,
 * where Dgraph takes every failed read for an absent key; a stored value that does not decode is corruption, where
 * Dgraph panics or reads past it.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod persistent_hnsw_insert;
mod persistent_hnsw_search;
mod persistent_hnsw_vector_index;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;

use ultragraphdb_core_tok::{
    EF_CONSTRUCTION_OPT, EF_SEARCH_OPT, EXPONENT_OPT, MAX_LEVELS_OPT, METRIC_OPT,
};
use ultragraphdb_traits_index::{CacheType, Metric, OptionValue, Options};

use crate::errors::hnsw_error::HnswError;
use crate::traits::hnsw_float::HnswFloat;
use crate::types::similarity_type::SimilarityType;
use crate::types::vector_keys::VectorKeys;
use crate::utils::helper::{bytes_as_float_array, bytes_to_uint64};
use crate::utils::matrix::decode_uint64_matrix;

/// Dgraph's `persistentHNSW`: the HNSW index of one vector predicate in one graph, stored in posting lists at the keys
/// its [`VectorKeys`] name. Each node's neighbour lists, every level's, are one value under the node; the entry point
/// is the 8 big-endian bytes of its node's uid; the deleted nodes are a JSON array of uids. A node's vector is its
/// forward list's value. Level 0 is the sparsest; level `max_levels - 1` holds every node.
///
/// An index lives for one insert or one search, as Dgraph's does: its caches of neighbour lists and dead nodes are
/// filled by that operation and dropped with it.
#[derive(Debug)]
pub struct PersistentHnsw<T> {
    max_levels: usize,
    ef_construction: usize,
    ef_search: usize,
    keys: VectorKeys,
    sim_type: SimilarityType,
    node_all_edges: RefCell<HashMap<u64, Vec<Vec<u64>>>>,
    dead_nodes: RefCell<Option<HashSet<u64>>>,
    float: PhantomData<T>,
}

impl<T: HnswFloat> PersistentHnsw<T> {
    /// The index at `keys`, with options `o`: Dgraph's `Create` of the hnsw factory. Without options it has 3 levels, an `efConstruction` of 150, an `efSearch` of 90 and the euclidean
    /// metric; an `exponent` `e` sets each of `maxLevels`, `efConstruction` and `efSearch` not given to `e`, `50e` and
    /// `30e`.
    ///
    /// # Errors
    ///
    /// [`HnswError::FloatBits`] when `float_bits` is not `T`'s width; [`HnswError::Options`] for a level or breadth
    /// option that is not an int; [`HnswError::CannotCast`] for a metric option that is not a metric;
    /// [`HnswError::InvalidParameter`] for a level count or breadth below 1.
    pub fn new(keys: VectorKeys, o: &Options, float_bits: usize) -> Result<Self, HnswError> {
        if float_bits != T::BITS {
            return Err(HnswError::FloatBits(float_bits, T::BITS));
        }
        let (max_levels, ef_construction, ef_search, metric) = apply_options(o)?;
        Ok(Self {
            max_levels: at_least_one(MAX_LEVELS_OPT, max_levels)?,
            ef_construction: at_least_one(EF_CONSTRUCTION_OPT, ef_construction)?,
            ef_search: at_least_one(EF_SEARCH_OPT, ef_search)?,
            keys,
            sim_type: SimilarityType::new(metric),
            node_all_edges: RefCell::new(HashMap::new()),
            dead_nodes: RefCell::new(None),
            float: PhantomData,
        })
    }

    pub fn max_levels(&self) -> usize {
        self.max_levels
    }

    pub fn ef_construction(&self) -> usize {
        self.ef_construction
    }

    pub fn ef_search(&self) -> usize {
        self.ef_search
    }

    /// Where the index keeps its lists and reads its vectors.
    pub fn keys(&self) -> &VectorKeys {
        &self.keys
    }

    pub fn sim_type(&self) -> SimilarityType {
        self.sim_type
    }

    /// Dgraph's `getDataFromKeyWithCacheType`: the value of `key`, labelled `label` at `uid`, `None` for an empty
    /// value. An absence `c` reports is wrapped as fetching the posting list; a failed read is returned as it is.
    fn get_data<C: CacheType<Error = HnswError>>(
        &self,
        key: &[u8],
        label: &str,
        uid: u64,
        c: &C,
    ) -> Result<Option<Vec<u8>>, HnswError> {
        match c.get(key) {
            Ok(data) if data.is_empty() => Ok(None),
            Ok(data) => Ok(Some(data)),
            Err(e) if e.is_absence() => Err(HnswError::FetchingPostingList(e, label, uid)),
            Err(e) => Err(e),
        }
    }

    /// The value of `key`, `None` when absent, whether `c` reported the absence or an empty value.
    fn get_data_or_none<C: CacheType<Error = HnswError>>(
        &self,
        key: &[u8],
        label: &str,
        uid: u64,
        c: &C,
    ) -> Result<Option<Vec<u8>>, HnswError> {
        match self.get_data(key, label, uid, c) {
            Err(e) if e.is_fetching_posting_list() => Ok(None),
            other => other,
        }
    }

    /// Dgraph's `getVecFromUid`: the vector of `uid`.
    ///
    /// # Errors
    ///
    /// [`HnswError::NilVector`] when `uid` holds none, wrapping the absence `c` reported; a failed read; the store's
    /// corruption for a vector whose length is not a multiple of the float width.
    fn vec_from_uid<C: CacheType<Error = HnswError>>(
        &self,
        uid: u64,
        c: &C,
    ) -> Result<Vec<T>, HnswError> {
        let key = self.keys.vector(uid);
        match self.get_data(&key, self.keys.name(), uid, c) {
            Err(e) if e.is_fetching_posting_list() => Err(HnswError::NilVector(Some(e))),
            Err(e) => Err(e),
            Ok(None) => Err(HnswError::NilVector(None)),
            Ok(Some(data)) => bytes_as_float_array(&data).ok_or_else(|| {
                HnswError::corrupted(&key, format!("a vector of {} bytes", data.len()))
            }),
        }
    }

    /// The vector of `uid`, empty when it holds none: what Dgraph's callers that check the vector's length read.
    fn vec_or_empty<C: CacheType<Error = HnswError>>(
        &self,
        uid: u64,
        c: &C,
    ) -> Result<Vec<T>, HnswError> {
        match self.vec_from_uid(uid, c) {
            Err(e) if e.is_nil_vector() => Ok(Vec::new()),
            other => other,
        }
    }

    /// Dgraph's `fillNeighborEdges`: the neighbour lists of `uuid`, from the index's cache or from `c`, which fill
    /// the cache; `None` when `uuid` has none.
    fn fill_neighbor_edges<C: CacheType<Error = HnswError>>(
        &self,
        uuid: u64,
        c: &C,
    ) -> Result<Option<Vec<Vec<u64>>>, HnswError> {
        if let Some(edges) = self.node_all_edges.borrow().get(&uuid) {
            return Ok(Some(edges.clone()));
        }
        let key = self.keys.neighbours(uuid);
        let Some(data) = self.get_data_or_none(&key, &self.keys.neighbours_label(), uuid, c)?
        else {
            return Ok(None);
        };
        let edges = self.decode_edges(uuid, &data)?;
        self.node_all_edges.borrow_mut().insert(uuid, edges.clone());
        Ok(Some(edges))
    }

    /// The neighbour lists stored for `uuid`.
    fn decode_edges(&self, uuid: u64, data: &[u8]) -> Result<Vec<Vec<u64>>, HnswError> {
        decode_uint64_matrix(data)
            .map_err(|reason| HnswError::corrupted(&self.keys.neighbours(uuid), reason))
    }

    /// Dgraph's `PickStartNode`: the entry point and its vector, or the first node holding a vector when the entry
    /// point is absent or lost its vector.
    fn pick_start_node<C: CacheType<Error = HnswError>>(
        &self,
        c: &C,
    ) -> Result<(u64, Vec<T>), HnswError> {
        // An empty entry point reads as an absent one, where Dgraph panics decoding it.
        let data = match self.get_data(&self.keys.entry(), &self.keys.entry_label(), 1, c) {
            Err(e) if e.is_fetching_posting_list() => return self.calculate_new_entry_vec(c),
            Err(e) => return Err(e),
            Ok(None) => return self.calculate_new_entry_vec(c),
            Ok(Some(data)) => data,
        };
        let entry = self.entry_uid(&data)?;
        let start_vec = self.vec_or_empty(entry, c)?;
        if start_vec.is_empty() {
            return self.calculate_new_entry_vec(c);
        }
        Ok((entry, start_vec))
    }

    /// The uid an entry point's value names.
    fn entry_uid(&self, data: &[u8]) -> Result<u64, HnswError> {
        bytes_to_uint64(data).ok_or_else(|| {
            HnswError::corrupted(
                &self.keys.entry(),
                format!("an entry point of {} bytes", data.len()),
            )
        })
    }

    /// Dgraph's `calculateNewEntryVec`: the first node, in key order, holding a vector, and its vector.
    ///
    /// # Errors
    ///
    /// [`HnswError::EmptyTree`] when no node holds a vector; a failed read; corruption of a vector read.
    fn calculate_new_entry_vec<C: CacheType<Error = HnswError>>(
        &self,
        c: &C,
    ) -> Result<(u64, Vec<T>), HnswError> {
        let start_vec = RefCell::new(Vec::new());
        let corrupt = RefCell::new(false);
        let found = c.find(
            &self.keys.vector_prefix(),
            |value| match bytes_as_float_array::<T>(value) {
                Some(v) => {
                    let holds = !v.is_empty();
                    *start_vec.borrow_mut() = v;
                    holds
                }
                None => {
                    *corrupt.borrow_mut() = true;
                    true
                }
            },
        );
        let itr = match found {
            Err(e) if e.is_absence() => return Err(HnswError::EmptyTree(Some(e))),
            Err(e) => return Err(e),
            Ok(itr) => itr,
        };
        if corrupt.into_inner() {
            return Err(HnswError::corrupted(
                &self.keys.vector(itr),
                "a vector whose length is not a multiple of its float width",
            ));
        }
        if itr == 0 {
            return Err(HnswError::EmptyTree(None));
        }
        Ok((itr, start_vec.into_inner()))
    }
}

/// Dgraph's `applyOptions`: the levels, the breadths and the metric of `o`, an exponent setting those not given.
fn apply_options(o: &Options) -> Result<(i64, i64, i64, Metric), HnswError> {
    let mut o = o.clone();
    if o.specifies(EXPONENT_OPT) {
        // Dgraph reads the exponent ignoring a type error, which leaves its default.
        let exponent = o.get_opt::<i64>(EXPONENT_OPT).ok().flatten().unwrap_or(3);
        if !o.specifies(MAX_LEVELS_OPT) {
            o.set_opt(MAX_LEVELS_OPT, OptionValue::Int(exponent));
        }
        if !o.specifies(EF_CONSTRUCTION_OPT) {
            o.set_opt(
                EF_CONSTRUCTION_OPT,
                OptionValue::Int(exponent.wrapping_mul(50)),
            );
        }
        if !o.specifies(EF_SEARCH_OPT) {
            o.set_opt(EF_SEARCH_OPT, OptionValue::Int(exponent.wrapping_mul(30)));
        }
    }
    let max_levels = o.get_opt::<i64>(MAX_LEVELS_OPT)?.unwrap_or(3);
    let ef_construction = o.get_opt::<i64>(EF_CONSTRUCTION_OPT)?.unwrap_or(150);
    let ef_search = o.get_opt::<i64>(EF_SEARCH_OPT)?.unwrap_or(90);
    let metric = match o.get_interface_opt(METRIC_OPT) {
        None => Metric::Euclidean,
        Some(OptionValue::Metric(m)) => *m,
        Some(other) => return Err(HnswError::CannotCast(other.go_type())),
    };
    Ok((max_levels, ef_construction, ef_search, metric))
}

/// `value` as a count, refused below 1.
fn at_least_one(name: &'static str, value: i64) -> Result<usize, HnswError> {
    usize::try_from(value)
        .ok()
        .filter(|&v| v >= 1)
        .ok_or_else(|| HnswError::InvalidParameter(name, value))
}
