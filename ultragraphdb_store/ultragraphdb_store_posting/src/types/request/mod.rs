/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/lists.go (LocalCache: plists, getInternal, Get, GetFromDelta, SetIfAbsent)
 * and posting/oracle.go (Txn.Get, Txn.GetFromDelta, Txn.GetScalarList). Changed for ultragraphdb: translated to
 * Rust; a request owns its lists, so no lock guards them; a list first written without a store read is completed
 * from the store before it is read; a scalar list is read through the merging read like every list, where
 * Dgraph's `GetScalarList` reads the newest version with a point get; the deltas, max versions, conflict keys and
 * the pending-transaction table are not ported, the request's writes reaching the store only in its commit batch.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod request_drop;

use std::collections::BTreeMap;
use std::collections::btree_map::Entry as MapEntry;
use std::sync::Arc;

use ultragraphdb_proto_pb::pb::PostingList;
use ultragraphdb_traits_store::{Entry, ListCache, Store};

use crate::errors::posting_error::PostingError;
use crate::types::cached_list::CachedList;
use crate::types::list::List;
use crate::types::lists::Lists;

/// A list a request holds, and whether it was built without a store read.
#[derive(Debug, Clone)]
struct Held {
    list: List,
    blind: bool,
}

/// One request's working set: Dgraph's `LocalCache` of a transaction. Each list it touches is read at its read
/// timestamp with the writes of the requests before it in its batch stacked on top, and its edges write the list's
/// scratch layer. Nothing it writes is visible outside its batch until the batch is committed. It holds a read open
/// at its timestamp, which bounds the discard timestamp, until it is dropped or committed.
#[derive(Debug)]
pub struct Request<'a, S, C> {
    lists: &'a Lists<S, C>,
    read_ts: u64,
    held: BTreeMap<Vec<u8>, Held>,
    stacked: BTreeMap<Vec<u8>, Vec<Arc<PostingList>>>,
    entries: Vec<Entry>,
    versioned: Vec<Entry>,
    advance_epoch: bool,
}

impl<'a, S, C> Request<'a, S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    pub(crate) fn new(
        lists: &'a Lists<S, C>,
        read_ts: u64,
        stacked: BTreeMap<Vec<u8>, Vec<Arc<PostingList>>>,
    ) -> Self {
        lists.open_read(read_ts);
        Self {
            lists,
            read_ts,
            held: BTreeMap::new(),
            stacked,
            entries: Vec::new(),
            versioned: Vec::new(),
            advance_epoch: false,
        }
    }

    pub fn read_ts(&self) -> u64 {
        self.read_ts
    }

    /// The posting lists the request reads: their store, for a scan at the request's read timestamp, and their
    /// read gate.
    pub fn posting_lists(&self) -> &'a Lists<S, C> {
        self.lists
    }

    /// The list of `key` as this request sees it, read whole: Dgraph's `Get`. A list written blind is completed
    /// from the store first, its writes kept on top.
    pub fn get(&mut self, key: &[u8]) -> Result<&mut List, PostingError> {
        let (lists, read_ts, stacked) = (self.lists, self.read_ts, &self.stacked);
        match self.held.entry(key.to_vec()) {
            MapEntry::Occupied(held) if !held.get().blind => Ok(&mut held.into_mut().list),
            MapEntry::Occupied(mut held) => {
                let written = held.get().list.delta().cloned();
                let list = read_list(lists, stacked, key, read_ts)?.with_scratch(read_ts, written);
                *held.get_mut() = Held { list, blind: false };
                Ok(&mut held.into_mut().list)
            }
            MapEntry::Vacant(vacant) => {
                let list = read_list(lists, stacked, key, read_ts)?;
                Ok(&mut vacant.insert(Held { list, blind: false }).list)
            }
        }
    }

    /// The list of `key` to write without reading the store: Dgraph's `GetFromDelta`, which index maintenance uses
    /// for lists it only adds to. A later [`Request::get`] of the key completes it.
    pub fn get_from_delta(&mut self, key: &[u8]) -> &mut List {
        let (read_ts, stacked) = (self.read_ts, &self.stacked);
        let held = self.held.entry(key.to_vec()).or_insert_with(|| {
            let mut list = List::empty(key.to_vec());
            list.mutation_mut()
                .set_stacked(stacked.get(key).cloned().unwrap_or_default());
            list.set_ts(read_ts);
            Held { list, blind: true }
        });
        &mut held.list
    }

    /// The list of a scalar predicate's key: Dgraph's `GetScalarList`, read whole as [`Request::get`] reads it.
    pub fn get_scalar_list(&mut self, key: &[u8]) -> Result<&mut List, PostingError> {
        self.get(key)
    }

    /// Writes `entry`'s key at the request's commit timestamp, whatever version `entry` names: a schema or type
    /// entry, or a deletion, committed with the request's lists.
    pub fn add_entry(&mut self, entry: Entry) {
        self.entries.push(entry);
    }

    /// Writes `entry` at the version it names, in the request's commit batch: what a rebuild's swap writes, its built
    /// lists and their deletions at the rebuild's timestamp. The caller keeps the store's rule that no write lands
    /// below a deletion of its key.
    pub fn add_entry_at_version(&mut self, entry: Entry) {
        self.versioned.push(entry);
    }

    /// Makes the request's commit advance the cache's epoch under the writer lock, with its batch: what a write that
    /// changes lists outside their deltas needs, so that no cached list or fill read before it serves a read after it,
    /// and no rollup read before it is written.
    pub fn advance_epoch_on_commit(&mut self) {
        self.advance_epoch = true;
    }

    /// The keys of the lists the request read from the store: those it holds, less those it wrote blind and has not
    /// read since.
    pub fn read_keys(&self) -> impl Iterator<Item = &[u8]> {
        self.held
            .iter()
            .filter(|(_, h)| !h.blind)
            .map(|(k, _)| k.as_slice())
    }

    /// A request reading as this one does, at its read timestamp with the same writes of its batch stacked beneath,
    /// holding none of its lists: a part of the request applied apart, on another thread, and taken back with
    /// [`Request::absorb`].
    pub fn fork(&self) -> Request<'a, S, C> {
        Request::new(self.lists, self.read_ts, self.stacked.clone())
    }

    /// Takes the lists and entries of `other`, a fork of this request, into it. The two are meant to hold disjoint
    /// keys; a key both hold keeps this request's list unless only `other` wrote to it.
    pub fn absorb(&mut self, mut other: Request<'a, S, C>) {
        for (key, held) in std::mem::take(&mut other.held) {
            match self.held.entry(key) {
                MapEntry::Vacant(vacant) => {
                    vacant.insert(held);
                }
                MapEntry::Occupied(mut mine) => {
                    if mine.get().list.delta().is_none() && held.list.delta().is_some() {
                        mine.insert(held);
                    }
                }
            }
        }
        self.entries.append(&mut other.entries);
        self.versioned.append(&mut other.versioned);
        self.advance_epoch |= other.advance_epoch;
    }

    /// The keys and lists the request holds.
    pub fn lists(&self) -> impl Iterator<Item = (&[u8], &List)> {
        self.held.iter().map(|(k, h)| (k.as_slice(), &h.list))
    }

    /// The deltas the request writes, by key: every list whose scratch layer holds postings.
    pub fn deltas(&self) -> Vec<(Vec<u8>, PostingList)> {
        self.held
            .iter()
            .filter_map(|(key, h)| {
                let delta = h.list.delta()?;
                (!delta.postings.is_empty()).then(|| (key.clone(), delta.clone()))
            })
            .collect()
    }

    pub(crate) fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub(crate) fn versioned_entries(&self) -> &[Entry] {
        &self.versioned
    }

    pub(crate) fn advances_epoch(&self) -> bool {
        self.advance_epoch
    }
}

/// The list of `key` at `read_ts`, with the batch's earlier writes to it stacked on top.
fn read_list<S, C>(
    lists: &Lists<S, C>,
    stacked: &BTreeMap<Vec<u8>, Vec<Arc<PostingList>>>,
    key: &[u8],
    read_ts: u64,
) -> Result<List, PostingError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    let mut list = lists.read(key, read_ts)?;
    list.mutation_mut()
        .set_stacked(stacked.get(key).cloned().unwrap_or_default());
    list.set_ts(read_ts);
    Ok(list)
}
