/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cmp::Ordering;

use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_store_apply::Reads;

use crate::errors::txn_error::TxnError;
use crate::types::span::Span;
use crate::types::txn_limits::MAX_FRAME;

/// The bytes a key in a table accounts for besides its own: its span.
const SPAN_BYTES: usize = 8;

/// What a read-write transaction read, the read set the commit step validates it by: the lists it read whole, the
/// statements it tested (a list key and an object's encoding), the key ranges it scanned (a start, inclusive, and an
/// end, exclusive, or none for the last key), the dictionary keys it found absent, and its namespace's catalog range,
/// which is always in it.
///
/// Every key is stored once, in one byte arena the read set owns, and named by a u32 offset and length; the arena and
/// the tables are cleared by [`ReadSet::reset`] and keep their capacity, so a worker reuses one read set for every
/// transaction it runs. [`ReadSet::seal`] sorts each table and drops its duplicates once, at commit: a resolver walks
/// them in key order beside its history, as a merge join. Keys are compared exactly, byte for byte; nothing is
/// summarised.
///
/// The read set is bounded: [`ReadSet::bytes`] counts the arena's bytes and 8 bytes per key the tables name (a
/// statement or a range names two, an open end included), the catalog range included, and a push past the limit is
/// refused with [`TxnError::ReadSetTooLarge`] and adds nothing.
#[derive(Debug, Clone)]
pub struct ReadSet {
    namespace: u64,
    read_version: u64,
    limit: usize,
    arena: Vec<u8>,
    lists: Vec<Span>,
    statements: Vec<(Span, Span)>,
    ranges: Vec<(Span, Option<Span>)>,
    absent: Vec<Span>,
    catalog: (Span, Option<Span>),
    sealed: bool,
}

impl Default for ReadSet {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadSet {
    /// An empty read set of namespace 0 at read version 0, its limit the default read-set bound.
    pub fn new() -> Self {
        Self::with_capacity(0, 0)
    }

    /// An empty read set as [`ReadSet::new`] makes it, its arena holding `bytes` and its tables `keys` entries each
    /// before they grow.
    pub fn with_capacity(bytes: usize, keys: usize) -> Self {
        let empty = Span { at: 0, len: 0 };
        let mut set = Self {
            namespace: 0,
            read_version: 0,
            limit: MAX_FRAME,
            arena: Vec::with_capacity(bytes),
            lists: Vec::with_capacity(keys),
            statements: Vec::with_capacity(keys),
            ranges: Vec::with_capacity(keys),
            absent: Vec::with_capacity(keys),
            catalog: (empty, None),
            sealed: false,
        };
        set.reset(0, 0, MAX_FRAME);
        set
    }

    /// Clears the read set for a transaction of `namespace` reading at `read_version`, bounded by `limit` bytes, and
    /// records the namespace's catalog range. Its buffers keep their capacity.
    pub fn reset(&mut self, namespace: u64, read_version: u64, limit: usize) {
        self.namespace = namespace;
        self.read_version = read_version;
        self.limit = limit.min(u32::MAX as usize);
        self.arena.clear();
        self.lists.clear();
        self.statements.clear();
        self.ranges.clear();
        self.absent.clear();
        self.sealed = false;
        // The catalog keyspace's prefix of the namespace, and the first key past every key under it.
        let mut prefix = [0u8; 9];
        prefix[0] = Keyspace::Catalog.byte();
        prefix[1..].copy_from_slice(&namespace.to_be_bytes());
        let start = self.add(&prefix);
        let end = prefix.iter().rposition(|b| *b != 0xff).map(|last| {
            let mut next = prefix;
            next[last] += 1;
            self.add(&next[..=last])
        });
        self.catalog = (start, end);
    }

    /// The bytes `arena` bytes and `spans` spans more would take the read set to, refused past its limit.
    fn room(&self, arena: usize, spans: usize) -> Result<(), TxnError> {
        let size = self.bytes() + arena + SPAN_BYTES * spans;
        if size > self.limit {
            return Err(TxnError::ReadSetTooLarge(size, self.limit));
        }
        Ok(())
    }

    /// Appends `bytes` to the arena: their span. The limit keeps the arena within u32 offsets.
    fn add(&mut self, bytes: &[u8]) -> Span {
        let at = self.arena.len() as u32;
        self.arena.extend_from_slice(bytes);
        Span {
            at,
            len: bytes.len() as u32,
        }
    }

    /// Records the list of `key`, read whole.
    ///
    /// # Errors
    ///
    /// [`TxnError::ReadSetTooLarge`] when it would take the read set past its limit; nothing is added.
    pub fn push_list(&mut self, key: &[u8]) -> Result<(), TxnError> {
        self.room(key.len(), 1)?;
        let span = self.add(key);
        self.lists.push(span);
        self.sealed = false;
        Ok(())
    }

    /// Records the statement of `object` in the list of `key`, tested.
    ///
    /// # Errors
    ///
    /// As [`ReadSet::push_list`].
    pub fn push_statement(&mut self, key: &[u8], object: &[u8]) -> Result<(), TxnError> {
        self.room(key.len() + object.len(), 2)?;
        let entry = (self.add(key), self.add(object));
        self.statements.push(entry);
        self.sealed = false;
        Ok(())
    }

    /// Records the range from `start`, inclusive, to `end`, exclusive, or to the last key, scanned.
    ///
    /// # Errors
    ///
    /// As [`ReadSet::push_list`].
    pub fn push_range(&mut self, start: &[u8], end: Option<&[u8]>) -> Result<(), TxnError> {
        self.room(start.len() + end.map_or(0, <[u8]>::len), 2)?;
        let start = self.add(start);
        let end = end.map(|e| self.add(e));
        self.ranges.push((start, end));
        self.sealed = false;
        Ok(())
    }

    /// Records the dictionary key `key`, found absent.
    ///
    /// # Errors
    ///
    /// As [`ReadSet::push_list`].
    pub fn push_absent(&mut self, key: &[u8]) -> Result<(), TxnError> {
        self.room(key.len(), 1)?;
        let span = self.add(key);
        self.absent.push(span);
        self.sealed = false;
        Ok(())
    }

    /// Records everything `reads` holds, an execution's read set, reserving room for it once.
    ///
    /// # Errors
    ///
    /// [`TxnError::ReadSetTooLarge`] when it would take the read set past its limit; the read set is then as it was.
    pub fn gather(&mut self, reads: &Reads) -> Result<(), TxnError> {
        let keys = |keys: &[Vec<u8>]| keys.iter().map(Vec::len).sum::<usize>();
        let arena = keys(reads.lists())
            + keys(reads.absent())
            + reads
                .statements()
                .iter()
                .map(|(k, o)| k.len() + o.len())
                .sum::<usize>()
            + reads
                .ranges()
                .iter()
                .map(|(s, e)| s.len() + e.as_ref().map_or(0, Vec::len))
                .sum::<usize>();
        let spans = reads.lists().len()
            + reads.absent().len()
            + 2 * (reads.statements().len() + reads.ranges().len());
        self.room(arena, spans)?;
        self.arena.reserve(arena);
        self.lists.reserve(reads.lists().len());
        self.statements.reserve(reads.statements().len());
        self.ranges.reserve(reads.ranges().len());
        self.absent.reserve(reads.absent().len());
        for key in reads.lists() {
            let span = self.add(key);
            self.lists.push(span);
        }
        for (key, object) in reads.statements() {
            let entry = (self.add(key), self.add(object));
            self.statements.push(entry);
        }
        for (start, end) in reads.ranges() {
            let start = self.add(start);
            let end = end.as_deref().map(|e| self.add(e));
            self.ranges.push((start, end));
        }
        for key in reads.absent() {
            let span = self.add(key);
            self.absent.push(span);
        }
        self.sealed = false;
        Ok(())
    }

    /// Sorts every table in key order and drops its duplicates: ranges by start, then by end, an open end after every
    /// other. Pushing after sealing unseals it.
    pub fn seal(&mut self) {
        let arena = self.arena.as_slice();
        let key = |s: &Span| s.of(arena);
        let pair = |(a, b): &(Span, Span)| (a.of(arena), b.of(arena));
        let range = |a: &(Span, Option<Span>), b: &(Span, Option<Span>)| {
            key(&a.0).cmp(key(&b.0)).then_with(|| match (a.1, b.1) {
                (Some(x), Some(y)) => key(&x).cmp(key(&y)),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            })
        };
        self.lists.sort_unstable_by(|a, b| key(a).cmp(key(b)));
        self.lists.dedup_by(|a, b| key(a) == key(b));
        self.absent.sort_unstable_by(|a, b| key(a).cmp(key(b)));
        self.absent.dedup_by(|a, b| key(a) == key(b));
        self.statements
            .sort_unstable_by(|a, b| pair(a).cmp(&pair(b)));
        self.statements.dedup_by(|a, b| pair(a) == pair(b));
        self.ranges.sort_unstable_by(range);
        self.ranges.dedup_by(|a, b| range(a, b) == Ordering::Equal);
        self.sealed = true;
    }

    /// Whether the tables are sorted and free of duplicates, as [`ReadSet::seal`] leaves them.
    pub fn is_sealed(&self) -> bool {
        self.sealed
    }

    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// The version the transaction read at.
    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    /// The read set's bound, in bytes as [`ReadSet::bytes`] counts them.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// The bytes the read set accounts for: its arena's and 8 per key its tables name.
    pub fn bytes(&self) -> usize {
        let spans = self.lists.len()
            + self.absent.len()
            + 2 * (self.statements.len() + self.ranges.len() + 1);
        self.arena.len() + SPAN_BYTES * spans
    }

    /// The capacity of its buffers: the arena's in bytes, and the sum of the tables' in entries.
    pub fn capacity(&self) -> (usize, usize) {
        (
            self.arena.capacity(),
            self.lists.capacity()
                + self.statements.capacity()
                + self.ranges.capacity()
                + self.absent.capacity(),
        )
    }

    /// The keys of the lists read whole: in key order once sealed.
    pub fn lists(&self) -> impl Iterator<Item = &[u8]> {
        self.lists.iter().map(|s| s.of(&self.arena))
    }

    /// The statements tested, by list key and object: in key order once sealed.
    pub fn statements(&self) -> impl Iterator<Item = (&[u8], &[u8])> {
        self.statements
            .iter()
            .map(|(k, o)| (k.of(&self.arena), o.of(&self.arena)))
    }

    /// The ranges scanned, each a start and an end or none: in order once sealed.
    pub fn ranges(&self) -> impl Iterator<Item = (&[u8], Option<&[u8]>)> {
        self.ranges
            .iter()
            .map(|(s, e)| (s.of(&self.arena), e.map(|e| e.of(&self.arena))))
    }

    /// The dictionary keys found absent: in key order once sealed.
    pub fn absent(&self) -> impl Iterator<Item = &[u8]> {
        self.absent.iter().map(|s| s.of(&self.arena))
    }

    /// The namespace's catalog range, from its start, inclusive, to its end, exclusive.
    pub fn catalog(&self) -> (&[u8], Option<&[u8]>) {
        (
            self.catalog.0.of(&self.arena),
            self.catalog.1.map(|e| e.of(&self.arena)),
        )
    }
}
