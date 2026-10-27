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

use std::collections::btree_map::Entry as MapEntry;
use std::collections::{BTreeMap, BTreeSet};

use ultragraphdb_core_codec::PostingList;
use ultragraphdb_traits_store::{Entry, ListCache, Scan, Store, StoreSnapshot, VersionlessEntry};

use crate::errors::posting_error::PostingError;
use crate::types::cached_list::CachedList;
use crate::types::list::List;
use crate::types::lists::Lists;
use crate::types::stacked_layers::StackedLayers;
use crate::types::writes::Writes;

/// A list a request holds, whether it was built without a store read, and whether the request read it whole rather
/// than only to test one statement.
#[derive(Debug, Clone)]
struct Held {
    list: List,
    blind: bool,
    whole: bool,
}

/// One request's working set: Dgraph's `LocalCache` of a transaction. Each list it touches is read at its read
/// timestamp, with the writes of the requests before it in its batch stacked on top when it was begun from a
/// [`crate::Batch`], and its edges write the list's scratch layer. Nothing it writes is visible to another request
/// until it is committed: in its batch, or as [`Writes`] in a round ([`crate::Lists::commit_round`]). It holds a read open
/// at its timestamp, which bounds the discard timestamp, until it is dropped or committed.
#[derive(Debug)]
pub struct Request<'a, S, C> {
    lists: &'a Lists<S, C>,
    read_ts: u64,
    held: BTreeMap<Vec<u8>, Held>,
    /// The statements the request tested, by list key and object.
    tested: BTreeSet<(Vec<u8>, Vec<u8>)>,
    /// The key ranges the request scanned, each from its start, inclusive, to its end, exclusive, or to the last key.
    ranges: BTreeSet<(Vec<u8>, Option<Vec<u8>>)>,
    stacked: StackedLayers,
    entries: Vec<Entry>,
    versioned: Vec<Entry>,
    versionless: Vec<VersionlessEntry>,
    advance_epoch: bool,
    removed_records: Vec<u64>,
}

impl<'a, S, C> Request<'a, S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    pub(crate) fn new(lists: &'a Lists<S, C>, read_ts: u64, stacked: StackedLayers) -> Self {
        lists.open_read(read_ts);
        Self {
            lists,
            read_ts,
            held: BTreeMap::new(),
            tested: BTreeSet::new(),
            ranges: BTreeSet::new(),
            stacked,
            entries: Vec::new(),
            versioned: Vec::new(),
            versionless: Vec::new(),
            advance_epoch: false,
            removed_records: Vec::new(),
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
        self.fetch(key, true)
    }

    /// The list of `key` to write without reading the store: Dgraph's `GetFromDelta`, which index maintenance uses
    /// for lists it only adds to. A later [`Request::get`] of the key completes it.
    pub fn get_from_delta(&mut self, key: &[u8]) -> &mut List {
        let (read_ts, stacked) = (self.read_ts, &self.stacked);
        let held = self.held.entry(key.to_vec()).or_insert_with(|| {
            let mut list = List::empty(key.to_vec());
            list.mutation_mut().set_stacked(stacked.get(key));
            list.set_ts(read_ts);
            Held {
                list,
                blind: true,
                whole: false,
            }
        });
        &mut held.list
    }

    /// The list of a scalar predicate's key: Dgraph's `GetScalarList`, read whole as [`Request::get`] reads it.
    pub fn get_scalar_list(&mut self, key: &[u8]) -> Result<&mut List, PostingError> {
        self.get(key)
    }

    /// The list of `key` read to test whether it holds `object`, an object's encoding: read as [`Request::get`] reads
    /// it, recording the statement and not the list, so that a commit changing other objects of the list does not
    /// conflict with the test. A list the request reads whole as well is recorded as read whole.
    pub fn get_tested(&mut self, key: &[u8], object: &[u8]) -> Result<&mut List, PostingError> {
        self.tested.insert((key.to_vec(), object.to_vec()));
        self.fetch(key, false)
    }

    /// The statements the request tested, by list key and object, in key order.
    pub fn tested(&self) -> impl Iterator<Item = (&[u8], &[u8])> {
        self.tested
            .iter()
            .map(|(k, o)| (k.as_slice(), o.as_slice()))
    }

    /// The keys of the lists in the range from `start`, inclusive, to `end`, exclusive, or to the last key when `end`
    /// is `None`, in key order: those the store holds at the request's read timestamp and those the request wrote. The
    /// range is recorded as scanned.
    pub fn scan(
        &mut self,
        start: Vec<u8>,
        end: Option<Vec<u8>>,
    ) -> Result<Vec<Vec<u8>>, PostingError> {
        self.lists.gate(self.read_ts)?;
        let snapshot = self.lists.store().snapshot(self.read_ts)?;
        let range = Scan::range(start.clone(), end.clone());
        let mut keys = BTreeSet::new();
        for item in snapshot.scan(&range) {
            keys.insert(item?.key().to_vec());
        }
        keys.extend(
            self.written_keys()
                .filter(|k| range.selects(k))
                .map(<[u8]>::to_vec),
        );
        self.ranges.insert((start, end));
        Ok(keys.into_iter().collect())
    }

    /// Records the range from `start`, inclusive, to `end`, exclusive, or to the last key when `end` is `None`, as
    /// scanned: what a scan the caller makes through the store itself, such as a skip scan, reports.
    pub fn record_range(&mut self, start: Vec<u8>, end: Option<Vec<u8>>) {
        self.ranges.insert((start, end));
    }

    /// The key ranges the request scanned, each from its start, inclusive, to its end, exclusive, or to the last key
    /// when it has none, in order.
    pub fn ranges(&self) -> impl Iterator<Item = (&[u8], Option<&[u8]>)> {
        self.ranges
            .iter()
            .map(|(start, end)| (start.as_slice(), end.as_deref()))
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

    /// Writes `value` as the value of `key` in the keyspace without versions, in the request's commit batch: a term
    /// dictionary entry, written with the lists that name the term.
    pub fn add_versionless(&mut self, key: Vec<u8>, value: Vec<u8>) {
        self.versionless.push(VersionlessEntry::put(key, value));
    }

    /// The keys of the entries the request adds to its commit batch, at its commit timestamp or at their own versions.
    pub fn entry_keys(&self) -> impl Iterator<Item = &[u8]> {
        self.entries.iter().chain(&self.versioned).map(Entry::key)
    }

    /// The versionless writes the request adds to its commit batch, in order.
    pub fn versionless(&self) -> &[VersionlessEntry] {
        &self.versionless
    }

    /// Makes the request's commit advance the cache's epoch under the writer lock, with its batch: what a write that
    /// changes lists outside their deltas needs, so that no cached list or fill read before it serves a read after it,
    /// and no rollup read before it is written.
    pub fn advance_epoch_on_commit(&mut self) {
        self.advance_epoch = true;
    }

    /// Makes the request's commit batch remove the rebuild record `id`, atomically with the request's writes: what a
    /// rebuild's swap batch and failure batch do.
    pub fn remove_rebuild_record_on_commit(&mut self, id: u64) {
        self.removed_records.push(id);
    }

    /// The keys of the lists the request read whole from the store: those it holds, less those it wrote blind and has
    /// not read since, and those it read only to test a statement ([`Request::tested`]).
    pub fn read_keys(&self) -> impl Iterator<Item = &[u8]> {
        self.held
            .iter()
            .filter(|(_, h)| h.whole)
            .map(|(k, _)| k.as_slice())
    }

    /// A request reading as this one does, at its read timestamp with the same writes of its batch stacked beneath,
    /// holding none of its lists: a part of the request applied apart, on another thread, and taken back with
    /// [`Request::absorb`].
    pub fn fork(&self) -> Request<'a, S, C> {
        Request::new(self.lists, self.read_ts, self.stacked.clone())
    }

    /// Takes the lists, entries and reads of `other`, a fork of this request, into it. The two are meant to hold
    /// disjoint keys; a key both hold keeps this request's list unless only `other` wrote to it, and is read whole when
    /// either read it whole.
    pub fn absorb(&mut self, mut other: Request<'a, S, C>) {
        for (key, held) in std::mem::take(&mut other.held) {
            match self.held.entry(key) {
                MapEntry::Vacant(vacant) => {
                    vacant.insert(held);
                }
                MapEntry::Occupied(mut mine) => {
                    let whole = mine.get().whole || held.whole;
                    if mine.get().list.delta().is_none() && held.list.delta().is_some() {
                        mine.insert(held);
                    }
                    mine.get_mut().whole = whole;
                }
            }
        }
        self.tested.append(&mut other.tested);
        self.ranges.append(&mut other.ranges);
        self.entries.append(&mut other.entries);
        self.versioned.append(&mut other.versioned);
        self.versionless.append(&mut other.versionless);
        self.advance_epoch |= other.advance_epoch;
        self.removed_records.append(&mut other.removed_records);
    }

    /// What the request writes, apart from it: its deltas, entries, versionless writes, epoch mark and removed rebuild
    /// records, which the commit step writes at the version it gives the request. Its read is closed.
    pub fn into_writes(mut self) -> Writes {
        let deltas = std::mem::take(&mut self.held)
            .into_iter()
            .filter_map(|(key, held)| {
                let delta = held.list.delta()?.clone();
                (!delta.postings().is_empty()).then_some((key, delta))
            })
            .collect();
        Writes::new(
            deltas,
            std::mem::take(&mut self.entries),
            std::mem::take(&mut self.versioned),
            std::mem::take(&mut self.versionless),
            self.advance_epoch,
            std::mem::take(&mut self.removed_records),
        )
    }

    /// The keys and lists the request holds.
    pub fn lists(&self) -> impl Iterator<Item = (&[u8], &List)> {
        self.held.iter().map(|(k, h)| (k.as_slice(), &h.list))
    }

    /// The deltas the request writes, by key: every list whose scratch layer holds postings.
    pub fn deltas(&self) -> Vec<(Vec<u8>, PostingList)> {
        self.written()
            .map(|(key, delta)| (key.to_vec(), delta.clone()))
            .collect()
    }

    /// The keys of [`Request::deltas`], in the same order, without copying the deltas.
    pub fn written_keys(&self) -> impl Iterator<Item = &[u8]> {
        self.written().map(|(key, _)| key)
    }

    /// The deltas the request writes, by key, borrowed from the lists it holds.
    pub(crate) fn written(&self) -> impl Iterator<Item = (&[u8], &PostingList)> {
        self.held.iter().filter_map(|(key, h)| {
            let delta = h.list.delta()?;
            (!delta.postings().is_empty()).then_some((key.as_slice(), delta))
        })
    }

    /// The list of `key` as the request sees it, completed from the store when it was written blind, and marked read
    /// whole when `whole` holds.
    fn fetch(&mut self, key: &[u8], whole: bool) -> Result<&mut List, PostingError> {
        let (lists, read_ts, stacked) = (self.lists, self.read_ts, &self.stacked);
        let held = match self.held.entry(key.to_vec()) {
            MapEntry::Occupied(held) if !held.get().blind => held.into_mut(),
            MapEntry::Occupied(mut held) => {
                let written = held.get().list.delta().cloned();
                let list = read_list(lists, stacked, key, read_ts)?.with_scratch(read_ts, written);
                let was_whole = held.get().whole;
                *held.get_mut() = Held {
                    list,
                    blind: false,
                    whole: was_whole,
                };
                held.into_mut()
            }
            MapEntry::Vacant(vacant) => {
                let list = read_list(lists, stacked, key, read_ts)?;
                vacant.insert(Held {
                    list,
                    blind: false,
                    whole: false,
                })
            }
        };
        held.whole |= whole;
        Ok(&mut held.list)
    }
}

/// The list of `key` at `read_ts`, with the batch's earlier writes to it stacked on top.
fn read_list<S, C>(
    lists: &Lists<S, C>,
    stacked: &StackedLayers,
    key: &[u8],
    read_ts: u64,
) -> Result<List, PostingError>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    let mut list = lists.read(key, read_ts)?;
    list.mutation_mut().set_stacked(stacked.get(key));
    list.set_ts(read_ts);
    Ok(list)
}
