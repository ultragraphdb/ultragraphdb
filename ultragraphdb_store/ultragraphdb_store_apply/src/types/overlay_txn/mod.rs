/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod overlay_txn_txn;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ops::ControlFlow;

use ultragraphdb_core_terms::{EncodedTerm, TermView};
use ultragraphdb_store_hnsw::{HnswError, VectorKeys};
use ultragraphdb_store_posting::{CachedList, PostingError};
use ultragraphdb_store_quads::QuadStore;
use ultragraphdb_traits_store::{ListCache, Scan, Store, StoreSnapshot};

/// The vector index's transaction while a build makes an index afresh: the index's lists in memory, its own writes
/// alone, and each node's vector read from the store's forward lists at the build's timestamp. The lists the store
/// holds for the index are not read: the build replaces them.
pub(crate) struct OverlayTxn<'q, S, C> {
    quads: &'q QuadStore<S, C>,
    keys: VectorKeys,
    read_ts: u64,
    values: RefCell<BTreeMap<Vec<u8>, Vec<u8>>>,
}

impl<'q, S, C> OverlayTxn<'q, S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// The transaction of the index at `keys`, reading vectors at `read_ts`, holding `values` already.
    pub fn new(
        quads: &'q QuadStore<S, C>,
        keys: VectorKeys,
        read_ts: u64,
        values: BTreeMap<Vec<u8>, Vec<u8>>,
    ) -> Self {
        Self {
            quads,
            keys,
            read_ts,
            values: RefCell::new(values),
        }
    }

    pub fn keys(&self) -> &VectorKeys {
        &self.keys
    }

    /// The index's lists the transaction wrote, by key.
    pub fn into_values(self) -> BTreeMap<Vec<u8>, Vec<u8>> {
        self.values.into_inner()
    }

    /// The value of `key`: the index's own, or a node's vector elements, little-endian 32-bit floats.
    pub(crate) fn value(&self, key: &[u8]) -> Result<Vec<u8>, HnswError> {
        if let Some(v) = self.values.borrow().get(key) {
            return if v.is_empty() {
                Err(HnswError::NoValue())
            } else {
                Ok(v.clone())
            };
        }
        if self.keys.node_of(key).is_none() {
            return Err(HnswError::NoValue());
        }
        let list = self.quads.lists().read(key, self.read_ts)?;
        let mut first = None;
        list.iterate(self.read_ts, 0, |p| {
            first = Some(p.value.clone());
            Ok::<_, PostingError>(ControlFlow::Break(()))
        })?;
        first
            .and_then(|v| elements(&v))
            .ok_or_else(HnswError::NoValue)
    }

    /// The node of the first forward list under `prefix` whose vector passes `filter`, in key order.
    pub(crate) fn first_passing(
        &self,
        prefix: &[u8],
        filter: impl Fn(&[u8]) -> bool,
    ) -> Result<u64, HnswError> {
        let lists = self.quads.lists();
        lists.gate(self.read_ts)?;
        let snapshot = lists
            .store()
            .snapshot(self.read_ts)
            .map_err(PostingError::from)?;
        for item in snapshot.scan(&Scan::prefix(prefix.to_vec())) {
            let item = item.map_err(PostingError::from)?;
            let Some(node) = self.keys.node_of(item.key()) else {
                continue;
            };
            match self.value(item.key()) {
                Ok(v) if filter(&v) => return Ok(node),
                Ok(_) => {}
                Err(e) if e.is_absence() => {}
                Err(e) => return Err(e),
            }
        }
        Err(HnswError::KeyNotFound())
    }

    /// Writes `value` at `key`.
    pub(crate) fn write(&self, key: &[u8], value: &[u8]) {
        self.values
            .borrow_mut()
            .insert(key.to_vec(), value.to_vec());
    }
}

/// The elements of the vector `encoding` encodes, `None` for another term.
fn elements(encoding: &[u8]) -> Option<Vec<u8>> {
    match EncodedTerm::from_bytes(encoding).ok()?.view().ok()? {
        TermView::Vector(v) => Some(v.as_bytes().to_vec()),
        _ => None,
    }
}
