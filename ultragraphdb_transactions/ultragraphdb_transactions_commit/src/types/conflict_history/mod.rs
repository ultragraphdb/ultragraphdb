/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::VecDeque;

use ultragraphdb_protocol_wire::ConflictKind;
use ultragraphdb_transactions_txn::ReadSet;

use crate::types::run::Run;

/// The most emptied runs kept for reuse.
const POOL: usize = 8;

/// The record of committed writes the resolver validates against: each admitted transaction's conflict points (keys
/// written), conflict statements (a list key and an object changed) and conflict ranges (a first key and the key past
/// the last, or none for the last key), each at its commit version, held above the floor, the read version of the
/// oldest open read-write transaction.
///
/// **Layout.** The history is a sequence of sorted runs, oldest first: each admitted transaction's writes become a
/// run of their own, whose keys sit in one byte arena named by u32 spans, sorted and deduplicated, each key with the
/// newest version that wrote it; the newest run is merged into the one before it while that one is no larger, up to
/// a run of [`ConflictHistory::RUN_CAP`] entries, so the history holds a logarithmic number of small runs at its
/// young end and capped runs behind them. A check walks a sealed read set beside each run that holds a version above
/// the read version, as a merge join in which each step binary-searches the rest of the run. Keys are compared
/// exactly, byte for byte; nothing is summarised.
///
/// **What conflicts.** A read conflicts with a write above its read version as follows; a statement written also
/// counts as its list touched.
///
/// | Read | Point `k` | Statement `(k, o)` | Range |
/// |---|---|---|---|
/// | list `k` read whole, or dictionary key `k` found absent | same key | same list key | contains `k` |
/// | statement `(k, o)` tested | same key | same key and object | contains `k` |
/// | range scanned, or the catalog range | key inside | list key inside | overlaps |
///
/// The kinds are checked in the order list, statement, range, absent term, catalog, each against points and
/// statements; a conflict with a written range is reported as [`ConflictKind::Drop`] after them.
///
/// **Pruning and memory.** [`ConflictHistory::prune`] raises the floor and drops every run whose newest version is at
/// or below it, each in constant time, its buffers kept for reuse; a merge leaves out the entries at or below the
/// floor. [`ConflictHistory::bytes`] accounts the arenas and the entries held.
#[derive(Debug, Clone)]
pub struct ConflictHistory {
    runs: VecDeque<Run>,
    pool: Vec<Run>,
    floor: u64,
}

impl ConflictHistory {
    /// The most entries a merge makes a run hold, so that one merge copies a bounded amount.
    pub const RUN_CAP: usize = 16_384;

    /// An empty history pruned to `floor`: after a restart, the store's applied version, since the writes below it
    /// are no longer known.
    pub fn new(floor: u64) -> Self {
        Self {
            runs: VecDeque::new(),
            pool: Vec::new(),
            floor,
        }
    }

    /// Records the writes of a transaction admitted at `version`: its conflict points, statements and ranges, an end
    /// of `None` reaching past the last key.
    pub fn insert<'k>(
        &mut self,
        version: u64,
        points: impl IntoIterator<Item = &'k [u8]>,
        statements: impl IntoIterator<Item = (&'k [u8], &'k [u8])>,
        ranges: impl IntoIterator<Item = (&'k [u8], Option<&'k [u8]>)>,
    ) {
        let mut run = self.pool.pop().unwrap_or_default();
        run.fill(version, points, statements, ranges);
        if run.is_empty() {
            self.give(run);
            return;
        }
        self.runs.push_back(run);
        // Merge the newest run into the one before it while that one is no larger, within the cap.
        while let [.., prev, last] = self.runs.make_contiguous() {
            if prev.len() > last.len() || prev.len() + last.len() > Self::RUN_CAP {
                break;
            }
            let mut merged = self.pool.pop().unwrap_or_default();
            merged.merge(prev, last, self.floor);
            for _ in 0..2 {
                if let Some(old) = self.runs.pop_back() {
                    self.give(old);
                }
            }
            if merged.is_empty() {
                self.give(merged);
            } else {
                self.runs.push_back(merged);
            }
        }
    }

    /// Keeps `run`, emptied, for reuse, while the pool has room.
    fn give(&mut self, mut run: Run) {
        if self.pool.len() < POOL {
            run.clear();
            self.pool.push(run);
        }
    }

    /// The kind of the first read of `read_set` that a write above its read version conflicts with, `None` when no
    /// write does. The read set's tables must be sealed: the walk relies on their order.
    pub fn conflict(&self, read_set: &ReadSet) -> Option<ConflictKind> {
        let rv = read_set.read_version();
        let above = || self.runs.iter().filter(|run| run.newest() > rv);
        if above().any(|run| run.keys_written(read_set.lists(), rv)) {
            return Some(ConflictKind::List);
        }
        if above().any(|run| run.statements_written(read_set.statements(), rv)) {
            return Some(ConflictKind::Statement);
        }
        if above().any(|run| run.ranges_written(read_set.ranges(), rv)) {
            return Some(ConflictKind::Range);
        }
        if above().any(|run| run.keys_written(read_set.absent(), rv)) {
            return Some(ConflictKind::AbsentTerm);
        }
        if above().any(|run| run.ranges_written(std::iter::once(read_set.catalog()), rv)) {
            return Some(ConflictKind::Catalog);
        }
        above()
            .any(|run| run.ranges_over(read_set, rv))
            .then_some(ConflictKind::Drop)
    }

    /// Raises the floor to `floor`, never lowering it, and drops what only versions at or below it wrote.
    pub fn prune(&mut self, floor: u64) {
        self.floor = self.floor.max(floor);
        while self
            .runs
            .front()
            .is_some_and(|run| run.newest() <= self.floor)
        {
            if let Some(run) = self.runs.pop_front() {
                self.give(run);
            }
        }
    }

    /// The version below which a read is too old: what the history no longer holds.
    pub fn floor(&self) -> u64 {
        self.floor
    }

    /// The bytes the history holds: its arenas' bytes and 24 per entry, the spans it names and its versions. A statement
    /// also holds its list key as touched, one entry more.
    pub fn bytes(&self) -> usize {
        self.runs.iter().map(Run::bytes).sum()
    }

    /// The capacity the history's buffers hold, in the same units as [`ConflictHistory::bytes`], the runs kept for
    /// reuse included: what it has allocated.
    pub fn capacity(&self) -> usize {
        self.runs.iter().chain(&self.pool).map(Run::capacity).sum()
    }

    /// The number of runs.
    pub fn runs(&self) -> usize {
        self.runs.len()
    }

    /// The number of entries held: points, statements and ranges.
    pub fn len(&self) -> usize {
        self.runs.iter().map(Run::len).sum()
    }

    /// Whether the history holds no entry.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
