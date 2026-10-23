/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/oracle.go (viTxn, NewViTxn, GetValueFromPostingList), posting/lists.go
 * (viLocalCache, LocalCache.Find) and posting/list.go (indexEdgeToPbEdge). Changed for ultragraphdb: translated to
 * Rust; over a request of the local commit, which owns its lists, so locking a key does nothing; the value read is
 * the list's static value at the request's read timestamp each time, where Dgraph keeps it on the list until the
 * next write; `find` scans the store at the request's read timestamp, after the read gate, and reads each key's
 * value through the request.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod vi_txn_local_cache;
mod vi_txn_txn;

use std::cell::RefCell;

use ultragraphdb_core_keys::{data_key, parse};
use ultragraphdb_core_types::Value;
use ultragraphdb_proto_pb::pb::{DirectedEdge, directed_edge};
use ultragraphdb_store_posting::{
    BIT_EMPTY_POSTING, CachedList, DEL, PostingError, PostingErrorEnum, Request,
};
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

    /// Dgraph's `GetValueFromPostingList`: the first posting of the list's static value at the read timestamp.
    fn value(&self, key: &[u8]) -> Result<Vec<u8>, HnswError> {
        let mut request = self.request.borrow_mut();
        let read_ts = request.read_ts();
        let list = request.get(key)?;
        match list
            .static_value(read_ts)
            .and_then(|pl| pl.postings.first())
        {
            Some(p) if p.op != DEL => Ok(p.value.clone()),
            _ => Err(HnswError::NoValue()),
        }
    }

    /// Dgraph's `LocalCache.Find`: the uid of the first data key of the attribute `pred` names whose value passes
    /// `filter`, in key order, skipping keys whose newest version is an empty list.
    fn find_value(&self, pred: &[u8], filter: impl Fn(&[u8]) -> bool) -> Result<u64, HnswError> {
        let attr = String::from_utf8_lossy(pred);
        let start = data_key(&attr, 0)?;
        let prefix = start[..start.len() - 8].to_vec();
        let (lists, read_ts) = {
            let request = self.request.borrow();
            (request.posting_lists(), request.read_ts())
        };
        lists.gate(read_ts)?;
        let snapshot = lists
            .store()
            .snapshot(read_ts)
            .map_err(PostingError::from)?;
        for item in snapshot.scan(&Scan::prefix(prefix)) {
            let item = item.map_err(PostingError::from)?;
            let pk = parse(item.key())?;
            if pk.has_start_uid() || item.meta() & BIT_EMPTY_POSTING != 0 {
                continue;
            }
            let value = {
                let mut request = self.request.borrow_mut();
                let read_ts = request.read_ts();
                match request.get(item.key())?.value(read_ts) {
                    Ok(v) => match v.into_value() {
                        Some(Value::Bytes(b)) => b,
                        _ => continue,
                    },
                    Err(PostingError(PostingErrorEnum::NoValue)) => continue,
                    Err(e) => return Err(e.into()),
                }
            };
            if filter(&value) {
                return Ok(pk.uid());
            }
        }
        Err(HnswError::KeyNotFound())
    }

    /// Dgraph's `AddMutation`: writes `t` into the scratch layer of `key`'s list, as Dgraph's `indexEdgeToPbEdge`
    /// makes it an edge: a set of an untyped value.
    fn write(&self, key: &[u8], t: &KeyValue) -> Result<(), HnswError> {
        let edge = DirectedEdge {
            entity: t.entity(),
            attr: t.attr().to_string(),
            value: t.value().to_vec(),
            value_type: 0,
            op: directed_edge::Op::Set as i32,
            ..DirectedEdge::default()
        };
        self.request
            .borrow_mut()
            .get(key)?
            .add_mutation(&edge, None)?;
        Ok(())
    }
}
