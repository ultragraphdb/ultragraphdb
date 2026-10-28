/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::{ListCache, Store};

use crate::types::cached_list::CachedList;
use crate::types::lists::Lists;
use crate::types::request::Request;
use crate::types::stacked_layers::StackedLayers;
use crate::types::writes::Writes;

/// Requests committed together, in order, each at its own commit timestamp, in one store batch. A request begun
/// from the batch reads the writes of the requests already pushed to it: their deltas stack on the lists it reads,
/// newest last, above the state at its read timestamp.
#[derive(Debug)]
pub struct Batch<'a, S, C> {
    lists: &'a Lists<S, C>,
    requests: Vec<Request<'a, S, C>>,
    stacked: StackedLayers,
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
            stacked: StackedLayers::default(),
        }
    }

    /// A request reading at `read_ts` that sees the writes of the requests pushed so far.
    pub fn request(&self, read_ts: u64) -> Request<'a, S, C> {
        Request::new(self.lists, read_ts, self.stacked.view())
    }

    /// Adds `request` to the batch: its deltas stack under the requests begun after it.
    pub fn push(&mut self, request: Request<'a, S, C>) {
        self.stacked.push(request.deltas());
        self.requests.push(request);
    }

    /// Stacks the deltas of `writes`, a request's writes committed elsewhere, under the requests begun after it, as a
    /// pushed request's are: what the commit step's overlay of decided but unapplied rounds holds, read by its
    /// commit-time operations (7.5). The batch commits nothing of them.
    pub fn stack(&mut self, writes: &Writes) {
        self.stacked.push(writes.deltas().to_vec());
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
