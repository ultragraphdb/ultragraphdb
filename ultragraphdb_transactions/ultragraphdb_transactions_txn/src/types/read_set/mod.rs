/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cmp::Ordering;

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_store_apply::Reads;

use crate::errors::txn_error::TxnError;
use crate::types::span::Span;
use crate::types::txn_limits::MAX_FRAME;

/// The bytes a key in a table accounts for besides its own: its span.
pub(crate) const SPAN_BYTES: usize = 8;
/// A term key's length: its tag and a system ID's 16 bytes.
pub(crate) const TERM_KEY: usize = 1 + SystemId::BYTES;

/// What a read-write transaction read, the read set the commit step validates it by (transactions, "What a
/// transaction's reads record and what conflicts with them"): the lists it read whole, the statements it tested (a list
/// key and an object's encoding), the key ranges it scanned (a start, inclusive, and an end, exclusive, or none for the
/// last key), and the terms it found absent, each by its system ID's 16 big-endian bytes, recorded as no statement of
/// the term in the partition. The catalog is read through the version in effect at the commit epoch, which the
/// write-set record names, not through a key range.
///
/// Every key is stored once, in one byte arena the read set owns, and named by a u32 offset and length; the arena and
/// the tables are cleared by [`ReadSet::reset`] and keep their capacity, so a worker reuses one read set for every
/// transaction it runs. [`ReadSet::seal`] sorts each table and drops its duplicates once, at commit: a resolver walks
/// them in key order beside its history, as a merge join. Keys are compared exactly, byte for byte; nothing is
/// summarised.
///
/// The read set is bounded: [`ReadSet::bytes`] counts the arena's bytes and 8 bytes per key the tables name (a
/// statement or a range names two, an open end included), and a push past the limit is refused with
/// [`TxnError::read_set_too_large`] and adds nothing.
#[derive(Debug, Clone)]
pub struct ReadSet {
    tenant: u64,
    read_version: u64,
    limit: usize,
    arena: Vec<u8>,
    lists: Vec<Span>,
    statements: Vec<(Span, Span)>,
    ranges: Vec<(Span, Option<Span>)>,
    absent: Vec<Span>,
    sealed: bool,
}

impl Default for ReadSet {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadSet {
    /// An empty read set of tenant 0 at read version 0, its limit the default read-set bound.
    pub fn new() -> Self {
        Self::with_capacity(0, 0)
    }

    /// An empty read set as [`ReadSet::new`] makes it, its arena holding `bytes` and its tables `keys` entries each
    /// before they grow.
    pub fn with_capacity(bytes: usize, keys: usize) -> Self {
        let mut set = Self {
            tenant: 0,
            read_version: 0,
            limit: MAX_FRAME,
            arena: Vec::with_capacity(bytes),
            lists: Vec::with_capacity(keys),
            statements: Vec::with_capacity(keys),
            ranges: Vec::with_capacity(keys),
            absent: Vec::with_capacity(keys),
            sealed: false,
        };
        set.reset(0, 0, MAX_FRAME);
        set
    }

    /// Clears the read set for a transaction of `tenant` reading at `read_version`, bounded by `limit` bytes. Its
    /// buffers keep their capacity.
    pub fn reset(&mut self, tenant: u64, read_version: u64, limit: usize) {
        self.tenant = tenant;
        self.read_version = read_version;
        self.limit = limit.min(u32::MAX as usize);
        self.arena.clear();
        self.lists.clear();
        self.statements.clear();
        self.ranges.clear();
        self.absent.clear();
        self.sealed = false;
        debug_assert_eq!(self.bytes(), 0);
    }

    /// The bytes `arena` bytes and `spans` spans more would take the read set to, refused past its limit.
    fn room(&self, arena: usize, spans: usize) -> Result<(), TxnError> {
        let size = self.bytes() + arena + SPAN_BYTES * spans;
        if size > self.limit {
            return Err(TxnError::read_set_too_large(size, self.limit));
        }
        Ok(())
    }

    /// Appends `bytes` to the arena: their span. The limit keeps the arena within u32 offsets.
    fn add(&mut self, bytes: &[u8]) -> Span {
        // The cast is lossless: a `usize` holds 32 bits on every target the repository builds for.
        debug_assert!(self.arena.len() + bytes.len() <= u32::MAX as usize);
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
    /// [`TxnError::read_set_too_large`] when it would take the read set past its limit; nothing is added.
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

    /// The tag of a term key, a conflict key no store key takes: every store key begins with its keyspace's byte, and
    /// the keyspace without versions with 0xFF.
    pub const TERM_TAG: u8 = 0xFE;

    /// The conflict key of the term of `id`: [`ReadSet::TERM_TAG`] and the ID's 16 big-endian bytes. A term found
    /// absent is recorded by it, and a statement written names each of its terms by it in its record's conflict
    /// points, so the two meet exactly.
    #[must_use]
    pub fn term_key(id: SystemId) -> [u8; TERM_KEY] {
        let mut key = [Self::TERM_TAG; TERM_KEY];
        if let Some((_, rest)) = key.split_first_mut() {
            rest.copy_from_slice(&id.to_be_bytes());
        }
        debug_assert_eq!(key.first(), Some(&Self::TERM_TAG));
        key
    }

    /// Records the term of `id`, found absent: no statement of it in the partition.
    ///
    /// # Errors
    ///
    /// As [`ReadSet::push_list`].
    pub fn push_absent(&mut self, id: SystemId) -> Result<(), TxnError> {
        let key = Self::term_key(id);
        self.room(key.len(), 1)?;
        let span = self.add(&key);
        self.absent.push(span);
        self.sealed = false;
        Ok(())
    }

    /// Records everything `reads` holds, an execution's read set, reserving room for it once.
    ///
    /// # Errors
    ///
    /// [`TxnError::read_set_too_large`] when it would take the read set past its limit; the read set is then as it was.
    pub fn gather(&mut self, reads: &Reads) -> Result<(), TxnError> {
        let keys = |keys: &[Vec<u8>]| keys.iter().map(Vec::len).sum::<usize>();
        let arena = keys(reads.lists())
            + TERM_KEY * reads.absent().len()
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
        for id in reads.absent() {
            let span = self.add(&Self::term_key(*id));
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
        debug_assert!(self.lists.is_sorted_by(|a, b| key(a) < key(b)));
        debug_assert!(self.absent.is_sorted_by(|a, b| key(a) < key(b)));
        self.sealed = true;
    }

    /// Whether the tables are sorted and free of duplicates, as [`ReadSet::seal`] leaves them.
    pub fn is_sealed(&self) -> bool {
        self.sealed
    }

    /// The tenant the reads were made in.
    pub fn tenant(&self) -> u64 {
        self.tenant
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
        let spans =
            self.lists.len() + self.absent.len() + 2 * (self.statements.len() + self.ranges.len());
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

    /// The terms found absent, each its system ID's 16 big-endian bytes: in order once sealed.
    pub fn absent(&self) -> impl Iterator<Item = &[u8]> {
        self.absent.iter().map(|s| s.of(&self.arena))
    }
}
