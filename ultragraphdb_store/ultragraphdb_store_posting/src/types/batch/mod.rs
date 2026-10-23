/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;
use std::sync::Arc;

use ultragraphdb_proto_pb::pb::PostingList;
use ultragraphdb_traits_store::{ListCache, Store};

use crate::types::cached_list::CachedList;
use crate::types::lists::Lists;
use crate::types::request::Request;

/// Requests committed together, in order, each at its own commit timestamp, in one store batch. A request begun
/// from the batch reads the writes of the requests already pushed to it: their deltas stack on the lists it reads,
/// newest last, above the state at its read timestamp. Stage 3 writes an apply step's requests as one batch.
#[derive(Debug)]
pub struct Batch<'a, S, C> {
    lists: &'a Lists<S, C>,
    requests: Vec<Request<'a, S, C>>,
    stacked: BTreeMap<Vec<u8>, Vec<Arc<PostingList>>>,
}

impl<'a, S, C> Batch<'a, S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    pub(crate) fn new(lists: &'a Lists<S, C>) -> Self {
        Self {
            lists,
            requests: Vec::new(),
            stacked: BTreeMap::new(),
        }
    }

    /// A request reading at `read_ts` that sees the writes of the requests pushed so far.
    pub fn request(&self, read_ts: u64) -> Request<'a, S, C> {
        Request::new(self.lists, read_ts, self.stacked.clone())
    }

    /// Adds `request` to the batch: its deltas stack under the requests begun after it.
    pub fn push(&mut self, request: Request<'a, S, C>) {
        for (key, delta) in request.deltas() {
            self.stacked.entry(key).or_default().push(Arc::new(delta));
        }
        self.requests.push(request);
    }

    pub fn requests(&self) -> &[Request<'a, S, C>] {
        &self.requests
    }

    pub fn len(&self) -> usize {
        self.requests.len()
    }

    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }

    pub(crate) fn into_requests(self) -> Vec<Request<'a, S, C>> {
        self.requests
    }
}
