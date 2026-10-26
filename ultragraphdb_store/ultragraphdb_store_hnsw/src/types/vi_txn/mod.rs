/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/oracle.go (viTxn, NewViTxn, GetValueFromPostingList), posting/lists.go
 * (viLocalCache, LocalCache.Find) and posting/list.go (indexEdgeToPbEdge). Changed for ultragraphdb: translated to
 * Rust; over a request of the local commit, which owns its lists, so locking a key does nothing; the value read is
 * the list's static value at the request's read timestamp each time, where Dgraph keeps it on the list until the
 * next write; a node's vector is read from its forward list, whose value is the vector literal's encoding, its tag
 * and dimension stripped; `find` scans the forward lists under the prefix at the request's read timestamp, after the
 * read gate, and reads each key's value through the request.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod vi_txn_local_cache;
mod vi_txn_txn;

use std::cell::RefCell;
use std::ops::ControlFlow;

use ultragraphdb_core_keys::QuadKey;
use ultragraphdb_core_terms::{EncodedTerm, TermView};
use ultragraphdb_proto_pb::pb::{Posting, posting};
use ultragraphdb_store_posting::{BIT_EMPTY_POSTING, CachedList, DEL, PostingError, Request, SET};
use ultragraphdb_traits_index::KeyValue;
use ultragraphdb_traits_store::{ListCache, Scan, Store, StoreSnapshot};

use crate::errors::hnsw_error::HnswError;

/// Dgraph's `viTxn` and `viLocalCache`: the vector index's [`Txn`](ultragraphdb_traits_index::Txn) and
/// [`LocalCache`](ultragraphdb_traits_index::LocalCache) over a request of the local commit. The index reads a key's
/// value through the request, its earlier writes included, and writes its neighbour lists and entry point into the
/// request's scratch layers, which reach the store in the request's commit batch.
///
/// A key holding no value, or whose value is a delete, reads as [`HnswError::NoValue`]; a failed read is the posting
/// crate's error.
#[derive(Debug)]
pub struct ViTxn<'r, 'a, S, C> {
    request: RefCell<&'r mut Request<'a, S, C>>,
}

impl<'r, 'a, S, C> ViTxn<'r, 'a, S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// Dgraph's `NewViTxn`: the index's transaction over `request`.
    pub fn new(request: &'r mut Request<'a, S, C>) -> Self {
        Self {
            request: RefCell::new(request),
        }
    }

    /// The request's read timestamp.
    pub fn read_ts(&self) -> u64 {
        self.request.borrow().read_ts()
    }

    /// Dgraph's `GetValueFromPostingList`: the first posting of the list's static value at the read timestamp; for a
    /// forward list, the elements of the vector its value encodes. A list holding no value, or a forward list whose
    /// value is not a vector, holds none.
    fn value(&self, key: &[u8]) -> Result<Vec<u8>, HnswError> {
        let mut request = self.request.borrow_mut();
        let read_ts = request.read_ts();
        let list = request.get(key)?;
        if matches!(QuadKey::parse(key), Ok((QuadKey::Forward { .. }, _))) {
            // A forward list's newest delta may lead with the deletion of the vector it replaces: read it merged.
            let mut first = None;
            list.iterate(read_ts, 0, |p| {
                first = Some(p.value.clone());
                Ok::<_, PostingError>(ControlFlow::Break(()))
            })?;
            return first
                .and_then(|v| vector_elements(&v))
                .ok_or_else(HnswError::NoValue);
        }
        match list
            .static_value(read_ts)
            .and_then(|pl| pl.postings.first())
        {
            Some(p) if p.op != DEL => Ok(p.value.clone()),
            _ => Err(HnswError::NoValue()),
        }
    }

    /// Dgraph's `LocalCache.Find`: the node of the first forward list under `prefix` whose vector passes `filter`, in
    /// key order, skipping lists whose newest version is empty and lists holding no vector.
    fn find_value(&self, prefix: &[u8], filter: impl Fn(&[u8]) -> bool) -> Result<u64, HnswError> {
        let (lists, read_ts) = {
            let request = self.request.borrow();
            (request.posting_lists(), request.read_ts())
        };
        lists.gate(read_ts)?;
        let snapshot = lists
            .store()
            .snapshot(read_ts)
            .map_err(PostingError::from)?;
        for item in snapshot.scan(&Scan::prefix(prefix.to_vec())) {
            let item = item.map_err(PostingError::from)?;
            let Ok((QuadKey::Forward { subject, .. }, None)) = QuadKey::parse(item.key()) else {
                continue;
            };
            if item.meta() & BIT_EMPTY_POSTING != 0 {
                continue;
            }
            let value = match self.value(item.key()) {
                Ok(value) => value,
                Err(e) if e.is_absence() => continue,
                Err(e) => return Err(e),
            };
            if filter(&value) {
                return Ok(subject);
            }
        }
        Err(HnswError::KeyNotFound())
    }

    /// Dgraph's `AddMutation`: writes `t`'s value into the scratch layer of `key`'s list, as its one value.
    fn write(&self, key: &[u8], t: &KeyValue) -> Result<(), HnswError> {
        self.request
            .borrow_mut()
            .get(key)?
            .add_posting(index_posting(t.value().to_vec()), false);
        Ok(())
    }
}

/// The elements of the vector `encoding` encodes, little-endian 32-bit floats; `None` for another term.
pub(crate) fn vector_elements(encoding: &[u8]) -> Option<Vec<u8>> {
    let term = EncodedTerm::from_bytes(encoding).ok()?;
    match term.view().ok()? {
        TermView::Vector(v) => Some(v.as_bytes().to_vec()),
        _ => None,
    }
}

/// The posting of a vector-index list's one value, as Dgraph's `indexEdgeToPbEdge` makes the edge of an untyped value:
/// under the uid of a value that is no list's element, so a later value replaces it.
pub fn index_posting(value: Vec<u8>) -> Posting {
    Posting {
        uid: u64::MAX,
        value,
        posting_type: posting::PostingType::Value as i32,
        op: SET,
        ..Posting::default()
    }
}
