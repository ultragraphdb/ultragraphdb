/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The vector index's maintenance, ported from Dgraph's `posting/index.go` (`addIndexMutations` for a vector, its
//! dead list): a vector written to a predicate declared with a vector index is inserted into the index of its graph and
//! predicate after its forward list holds it, and a vector deleted marks its node dead. A node whose vector is written
//! again leaves the dead list, and a vector of another dimension than the entry point's is refused. While the
//! predicate's vector index is built, writes record the nodes whose vector they change.

use ultragraphdb_core_mapping::PredicateDeclaration;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_hnsw::{
    VectorKeys, ViTxn, add_dead_node, bytes_to_uint64, index_posting, parse_edges,
};
use ultragraphdb_store_posting::{CachedList, DEL};
use ultragraphdb_traits_index::VectorIndex;
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_write::ChangesetWrite;
use crate::types::changeset_write::changeset_write_triples::Resolved;
use crate::utils::lock::lock;
use crate::utils::vector_index::{hnsw, vector_of};

impl<S, C> ChangesetWrite<'_, S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// The keys of the vector index of `predicate` in `graph`.
    fn vector_keys(
        &self,
        declaration: &PredicateDeclaration,
        graph: u64,
        predicate: u64,
    ) -> VectorKeys {
        VectorKeys::new(
            declaration.predicate().as_str(),
            self.namespace(),
            graph,
            predicate,
        )
    }

    /// The single value of the vector-index list `key` as the write sees it, `None` when it holds none.
    fn index_value(&mut self, key: &[u8]) -> Result<Option<Vec<u8>>, ChangesetError> {
        let request = self.write.request();
        let read_ts = request.read_ts();
        let list = request.get(key)?;
        Ok(list
            .static_value(read_ts)
            .and_then(|pl| pl.postings.first())
            .filter(|p| p.op != DEL)
            .map(|p| p.value.clone()))
    }

    /// Writes `value` as the single value of the vector-index list `key`.
    fn set_index_value(&mut self, key: &[u8], value: Vec<u8>) -> Result<(), ChangesetError> {
        self.write
            .request()
            .get(key)?
            .add_posting(index_posting(value), false);
        Ok(())
    }

    /// Refuses a vector written to a predicate with a vector index whose dimension is not that of the entry point's
    /// vector, when the index of its graph holds one.
    pub(crate) fn check_dimension(
        &mut self,
        r: &Resolved,
        declaration: &PredicateDeclaration,
    ) -> Result<(), ChangesetError> {
        let (Some(_), Some(written)) = (declaration.vector_index(), vector_of(&r.object)) else {
            return Ok(());
        };
        let keys = self.vector_keys(declaration, r.graph, r.predicate);
        let Some(entry) = self
            .index_value(&keys.entry())?
            .and_then(|v| bytes_to_uint64(&v))
        else {
            return Ok(());
        };
        let held = self
            .write
            .objects(r.graph, entry, r.predicate)?
            .iter()
            .find_map(|(o, _)| vector_of(o))
            .map(|v| v.len());
        match held {
            Some(held) if held != written.len() => Err(ChangesetError::Dimension(
                declaration.predicate().as_str(),
                held,
                written.len(),
            )),
            _ => Ok(()),
        }
    }

    /// Inserts `object`, a vector the subject's forward list now holds, into the vector index of its graph and
    /// predicate, its node out of the dead list.
    pub(crate) fn index_vector(
        &mut self,
        graph: u64,
        subject: u64,
        predicate: u64,
        declaration: &PredicateDeclaration,
        object: &EncodedTerm,
    ) -> Result<(), ChangesetError> {
        let Some(vector) = vector_of(object) else {
            return Ok(());
        };
        if self.vector_built == Some(predicate) {
            self.touched.insert((graph, subject));
        }
        let Some(spec) = declaration.vector_index() else {
            return Ok(());
        };
        let keys = self.vector_keys(declaration, graph, predicate);
        self.unmark_dead(&keys, subject)?;
        let index = hnsw(spec, keys)?;
        let draws = self.draws;
        let txn = ViTxn::new(self.write.request());
        let mut draw = || lock(draws).next_f64();
        VectorIndex::insert(&index, &txn, subject, &vector, &mut draw)?;
        Ok(())
    }

    /// Marks the subject dead in the vector index of `object`'s graph and predicate, `object` a vector it no longer
    /// holds.
    pub(crate) fn unindex_vector(
        &mut self,
        graph: u64,
        subject: u64,
        predicate: u64,
        declaration: &PredicateDeclaration,
        object: &EncodedTerm,
    ) -> Result<(), ChangesetError> {
        if vector_of(object).is_none() {
            return Ok(());
        }
        if self.vector_built == Some(predicate) {
            self.touched.insert((graph, subject));
        }
        if declaration.vector_index().is_none() {
            return Ok(());
        }
        let keys = self.vector_keys(declaration, graph, predicate);
        let existing = self.index_value(&keys.dead())?;
        let listed = match &existing {
            Some(v) => parse_edges(&String::from_utf8_lossy(v))?,
            None => Vec::new(),
        };
        if listed.contains(&subject) {
            return Ok(());
        }
        let dead = add_dead_node(existing.as_deref(), subject)?;
        self.set_index_value(&keys.dead(), dead)
    }

    /// Removes `node` from the dead list at `keys`, when it is listed.
    fn unmark_dead(&mut self, keys: &VectorKeys, node: u64) -> Result<(), ChangesetError> {
        let Some(existing) = self.index_value(&keys.dead())? else {
            return Ok(());
        };
        let listed = parse_edges(&String::from_utf8_lossy(&existing))?;
        if !listed.contains(&node) {
            return Ok(());
        }
        let kept: Vec<String> = listed
            .iter()
            .filter(|&&u| u != node)
            .map(u64::to_string)
            .collect();
        let value = format!("[{}]", kept.join(",")).into_bytes();
        self.set_index_value(&keys.dead(), value)
    }
}
