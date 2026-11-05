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
use std::ops::Bound;

use ultragraphdb_core_codec::PostingList;
use ultragraphdb_traits_store::{Entry, ListCache, Scan, Store, VersionlessEntry};

use crate::errors::posting_error::PostingError;
use crate::traits::list_source::ListSource;
use crate::types::cached_list::CachedList;
use crate::types::list::List;
use crate::types::lists::Lists;
use crate::types::stacked_layers::StackedLayers;
use crate::types::walk_bounds::WalkBounds;
use crate::types::walk_end::WalkEnd;
use crate::types::walk_step::WalkStep;
use crate::types::writes::Writes;
use crate::utils::walk_lists::walk_lists;

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
    /// The bytes of `ranges`, kept as they change.
    range_bytes: usize,
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
            range_bytes: 0,
            stacked,
            entries: Vec::new(),
            versioned: Vec::new(),
            versionless: Vec::new(),
            advance_epoch: false,
            removed_records: Vec::new(),
        }
    }

    /// The request's read timestamp.
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
        let _first_test: bool = self.tested.insert((key.to_vec(), object.to_vec()));
        self.fetch(key, false)
    }

    /// The list of `key` as the request's snapshot holds it: at its read timestamp with the writes of the requests
    /// before it in its batch beneath, without the request's own writes, and recorded as read nowhere. What a write
    /// comparing its net effect with its snapshot reads.
    pub fn snapshot(&self, key: &[u8]) -> Result<List, PostingError> {
        match self.held.get(key) {
            Some(held) if !held.blind => {
                let mut list = held.list.clone();
                list.mutation_mut().clear_current_entries();
                Ok(list)
            }
            Some(_) | None => read_list(self.lists, &self.stacked, key, self.read_ts),
        }
    }

    /// Drops every delta the request wrote, keeping each list it read, with its batch's writes beneath, and every
    /// read it recorded: what a write deriving its keys again from its net effect starts from, so that it reads its
    /// snapshot and its read set stays whole. A list written without a read is dropped with its delta.
    pub fn discard_deltas(&mut self) {
        self.held.retain(|_, held| !held.blind);
        for held in self.held.values_mut() {
            held.list.mutation_mut().clear_current_entries();
        }
    }

    /// The statements the request tested, by list key and object, in key order.
    pub fn tested(&self) -> impl Iterator<Item = (&[u8], &[u8])> {
        self.tested
            .iter()
            .map(|(k, o)| (k.as_slice(), o.as_slice()))
    }

    /// Walks the lists in the range from `start`, inclusive, to `end`, exclusive, or to the last key when `end` is `None`, in
    /// key order: those the store holds at the request's read timestamp and those the request wrote, each given to `visit`, which
    /// says what it made of the key, as far as `bounds` lets it. Nothing is held and no list is read whole, so a scan of a million
    /// lists costs the request one range and no memory. What it covered is recorded as scanned: the whole range; stopped at the
    /// limit or paused, from `start` to just past its last key, nothing when it gave nothing; yielded, from `start` to where it
    /// goes on, since the keys it read gave nothing and a later write into them conflicts. A span touching the one before extends
    /// it. How the walk ended.
    ///
    /// # Errors
    ///
    /// The read gate's and the store's errors; whatever `visit` fails with. Nothing is recorded.
    pub fn span<E, V>(
        &mut self,
        start: Vec<u8>,
        end: Option<Vec<u8>>,
        bounds: WalkBounds,
        visit: V,
    ) -> Result<WalkEnd, E>
    where
        E: From<PostingError>,
        V: FnMut(&[u8], &Self) -> Result<WalkStep, E>,
    {
        self.lists.gate(self.read_ts)?;
        let snapshot = self
            .lists
            .store()
            .snapshot(self.read_ts)
            .map_err(PostingError::from)?;
        let range = Scan::range(start.clone(), end.clone());
        let walked = walk_lists(&snapshot, &range, &*self, self.read_ts, bounds, visit)?;
        match (&walked, walked.past_last()) {
            (WalkEnd::Exhausted, _) => self.record_range(start, end),
            (
                WalkEnd::Stopped { .. } | WalkEnd::Paused { .. } | WalkEnd::Yielded { .. },
                Some(past),
            ) => {
                self.record_range(start, Some(past));
            }
            (WalkEnd::Stopped { .. } | WalkEnd::Paused { .. } | WalkEnd::Yielded { .. }, None) => {}
        }
        Ok(walked)
    }

    /// Records the range from `start`, inclusive, to `end`, exclusive, or to the last key when `end` is `None`, as
    /// scanned: what a scan the caller makes through the store itself, such as a skip scan, reports. A range the one before
    /// it reaches extends that one, so a long scan holds one range however many spans it took.
    pub fn record_range(&mut self, start: Vec<u8>, end: Option<Vec<u8>>) {
        let first_at_start = (start.clone(), None);
        let before = self
            .ranges
            .range(&first_at_start..)
            .next()
            .filter(|(from, _)| *from == start)
            .or_else(|| self.ranges.range(..&first_at_start).next_back())
            .filter(|(_, reach)| reach.as_ref().is_none_or(|reach| *reach >= start))
            .cloned();
        let range = match before {
            Some(previous) => {
                let _removed: bool = self.ranges.remove(&previous);
                self.range_bytes -= range_size(&previous);
                let reach = match (previous.1, end) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (_, _) => None,
                };
                (previous.0, reach)
            }
            None => (start, end),
        };
        let size = range_size(&range);
        if self.ranges.insert(range) {
            self.range_bytes += size;
        }
    }

    /// The bytes of the key ranges the request scanned, counted as they are recorded, so asking costs nothing per range.
    pub fn range_bytes(&self) -> usize {
        self.range_bytes
    }

    /// How many key ranges the request scanned.
    pub fn range_count(&self) -> usize {
        self.ranges.len()
    }

    /// How many lists the request holds: what a scan leaves unchanged.
    pub fn held_count(&self) -> usize {
        self.held.len()
    }

    /// The key ranges the request scanned, each from its start, inclusive, to its end, exclusive, or to the last key
    /// when it has none, in order.
    pub fn ranges(&self) -> impl Iterator<Item = (&[u8], Option<&[u8]>)> {
        self.ranges
            .iter()
            .map(|(start, end)| (start.as_slice(), end.as_deref()))
    }

    /// Takes `deltas` as the request's own writes to their lists, as though it had written them blind: a list is
    /// completed from the store, with the batch's layers beneath, when it is read, the delta kept on top, and later
    /// edges add to it. What a commit-time operation continues a transaction's writes with, so that the two leave one
    /// delta per list (7.5). A list the request already holds is left as it is.
    pub fn seed_deltas(&mut self, deltas: &[(Vec<u8>, PostingList)]) {
        let (read_ts, stacked) = (self.read_ts, &self.stacked);
        for (key, delta) in deltas {
            let _held: &mut Held = self.held.entry(key.clone()).or_insert_with(|| {
                let mut list = List::empty(key.clone()).with_scratch(read_ts, Some(delta.clone()));
                list.mutation_mut().set_stacked(stacked.get(key));
                Held {
                    list,
                    blind: true,
                    whole: false,
                }
            });
        }
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

    /// Adds `entry`, a put or a removal of the keyspace without versions, to the request's commit batch, as it is.
    pub fn add_versionless_entry(&mut self, entry: VersionlessEntry) {
        self.versionless.push(entry);
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
                    let _taken: &mut Held = vacant.insert(held);
                }
                MapEntry::Occupied(mut mine) => {
                    let whole = mine.get().whole || held.whole;
                    if mine.get().list.delta().is_none() && held.list.delta().is_some() {
                        let _unwritten: Held = mine.insert(held);
                    }
                    mine.get_mut().whole = whole;
                }
            }
        }
        self.tested.append(&mut other.tested);
        self.ranges.append(&mut other.ranges);
        self.range_bytes = self.ranges.iter().map(range_size).sum();
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
                let mut delta = held.list.delta()?.clone();
                // In posting order, uid then value, whatever order the write made them in: the same writes give the
                // same delta, so a logical record's replay derives it byte for byte (write-ahead-log, "A round's
                // record is logical"). Readers sort a delta's postings themselves.
                delta
                    .postings_mut()
                    .sort_by(|a, b| a.uid().cmp(&b.uid()).then_with(|| a.value().cmp(b.value())));
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

/// The bytes a recorded range holds.
fn range_size((start, end): &(Vec<u8>, Option<Vec<u8>>)) -> usize {
    start.len() + end.as_ref().map_or(0, Vec::len)
}

impl<S, C> ListSource for Request<'_, S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// The keys the request wrote in the range.
    fn added_keys<'a>(
        &'a self,
        start: &'a [u8],
        end: Option<&'a [u8]>,
    ) -> impl Iterator<Item = &'a [u8]> {
        let upper = end.map_or(Bound::Unbounded, Bound::Excluded);
        self.held
            .range::<[u8], _>((Bound::Included(start), upper))
            .filter(|(_, held)| {
                held.list
                    .delta()
                    .is_some_and(|delta| !delta.postings().is_empty())
            })
            .map(|(key, _)| key.as_slice())
    }

    /// A held list as held; a list written blind from the store with its delta on top; any other from the store. The
    /// request's own timestamp reads its writes, so `version` is the request's read timestamp.
    fn with_list<R>(
        &self,
        key: &[u8],
        version: u64,
        read: impl FnOnce(&List) -> R,
    ) -> Result<R, PostingError> {
        match self.held.get(key) {
            Some(held) if !held.blind => Ok(read(&held.list)),
            Some(held) => {
                let written = held.list.delta().cloned();
                let list = read_list(self.lists, &self.stacked, key, version)?
                    .with_scratch(version, written);
                Ok(read(&list))
            }
            None => Ok(read(&read_list(self.lists, &self.stacked, key, version)?)),
        }
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
