/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod run_check;
mod run_merge;

use crate::types::span::Span;

/// The bytes an entry accounts for besides its keys: its spans and its versions.
pub(crate) const ENTRY_BYTES: usize = 24;

/// A key written whole (`whole`) or as the list of a statement written (`touched`), each the newest version that did
/// so, 0 for none: commit versions are at least 2.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Point {
    pub key: Span,
    pub whole: u64,
    pub touched: u64,
}

impl Point {
    /// The newest version that wrote the key in any way.
    pub fn newest(&self) -> u64 {
        self.whole.max(self.touched)
    }
}

/// A statement written: its list key, its object and the newest version that wrote it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Statement {
    pub key: Span,
    pub object: Span,
    pub version: u64,
}

/// A range written: its first key, the key past its last or none, and its version.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Written {
    pub start: Span,
    pub end: Option<Span>,
    pub version: u64,
}

/// One sorted run of the conflict history: its keys in one arena, its points sorted by key and its statements by key
/// and object, each held once with the newest version that wrote it, and its ranges. Cleared, it keeps its capacity.
///
/// **Key ranges** (approved 2026-10-28). A check skips a run whose keys cannot hold a read's key: the points' keys lie
/// between the first and the last point, which the sort gives with nothing stored, and every statement's list key is
/// among the points; the ranges, unsorted, are bounded by `ranges_low`, their lowest start, and `ranges_high`, their
/// highest end or `None` past every key, set by [`Run::fill`] and [`Run::merge`]. The bounds only skip: a key within
/// them is still compared exactly.
#[derive(Debug, Clone, Default)]
pub(crate) struct Run {
    arena: Vec<u8>,
    points: Vec<Point>,
    statements: Vec<Statement>,
    ranges: Vec<Written>,
    ranges_low: Span,
    ranges_high: Option<Span>,
    newest: u64,
}

impl Run {
    /// Empties the run, keeping its buffers.
    pub fn clear(&mut self) {
        self.arena.clear();
        self.points.clear();
        self.statements.clear();
        self.ranges.clear();
        self.ranges_low = Span::default();
        self.ranges_high = None;
        self.newest = 0;
    }

    /// The keys of the first and the last point, `None` for a run without one.
    pub fn point_bounds(&self) -> Option<(&[u8], &[u8])> {
        let first = self.points.first()?;
        let last = self.points.last()?;
        Some((first.key.of(&self.arena), last.key.of(&self.arena)))
    }

    /// The lowest start and the highest end of the ranges, an end of `None` past every key; `None` for a run without a
    /// range.
    pub fn range_bounds(&self) -> Option<(&[u8], Option<&[u8]>)> {
        if self.ranges.is_empty() {
            return None;
        }
        Some((
            self.ranges_low.of(&self.arena),
            self.ranges_high.map(|h| h.of(&self.arena)),
        ))
    }

    /// Sets the ranges' bounds from the ranges held.
    fn bound_ranges(&mut self) {
        let arena = self.arena.as_slice();
        let Some(first) = self.ranges.first() else {
            return;
        };
        let (mut low, mut high) = (first.start, first.end);
        for w in &self.ranges {
            if w.start.of(arena) < low.of(arena) {
                low = w.start;
            }
            high = match (high, w.end) {
                (Some(h), Some(e)) if e.of(arena) > h.of(arena) => Some(e),
                (Some(h), Some(_)) => Some(h),
                _ => None,
            };
        }
        self.ranges_low = low;
        self.ranges_high = high;
    }

    /// The number of entries: points, statements and ranges.
    pub fn len(&self) -> usize {
        self.points.len() + self.statements.len() + self.ranges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The newest version the run holds.
    pub fn newest(&self) -> u64 {
        self.newest
    }

    /// The arena's bytes and [`ENTRY_BYTES`] per entry.
    pub fn bytes(&self) -> usize {
        self.arena.len() + ENTRY_BYTES * self.len()
    }

    /// The same over the buffers' capacity.
    pub fn capacity(&self) -> usize {
        self.arena.capacity()
            + ENTRY_BYTES
                * (self.points.capacity() + self.statements.capacity() + self.ranges.capacity())
    }

    /// Appends `bytes` to the arena: their span. A run holds a transaction's or two runs' keys, far below u32 offsets.
    fn add(&mut self, bytes: &[u8]) -> Span {
        let at = self.arena.len() as u32;
        self.arena.extend_from_slice(bytes);
        Span {
            at,
            len: bytes.len() as u32,
        }
    }

    /// Fills the run, cleared, with one transaction's writes at `version`, sorted and each held once.
    pub fn fill<'k>(
        &mut self,
        version: u64,
        points: impl IntoIterator<Item = &'k [u8]>,
        statements: impl IntoIterator<Item = (&'k [u8], &'k [u8])>,
        ranges: impl IntoIterator<Item = (&'k [u8], Option<&'k [u8]>)>,
    ) {
        self.clear();
        for key in points {
            let key = self.add(key);
            self.points.push(Point {
                key,
                whole: version,
                touched: 0,
            });
        }
        for (key, object) in statements {
            let key = self.add(key);
            let object = self.add(object);
            self.statements.push(Statement {
                key,
                object,
                version,
            });
            self.points.push(Point {
                key,
                whole: 0,
                touched: version,
            });
        }
        for (start, end) in ranges {
            let start = self.add(start);
            let end = end.map(|e| self.add(e));
            self.ranges.push(Written {
                start,
                end,
                version,
            });
        }
        let arena = self.arena.as_slice();
        self.points
            .sort_unstable_by(|a, b| a.key.of(arena).cmp(b.key.of(arena)));
        self.points.dedup_by(|later, kept| {
            let same = later.key.of(arena) == kept.key.of(arena);
            if same {
                kept.whole = kept.whole.max(later.whole);
                kept.touched = kept.touched.max(later.touched);
            }
            same
        });
        let pair = |s: &Statement| (s.key.of(arena), s.object.of(arena));
        self.statements
            .sort_unstable_by(|a, b| pair(a).cmp(&pair(b)));
        self.statements
            .dedup_by(|later, kept| pair(later) == pair(kept));
        self.bound_ranges();
        if !self.is_empty() {
            self.newest = version;
        }
    }
}
