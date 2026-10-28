/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_transactions_txn::ReadSet;

use crate::types::run::Run;

/// Whether `key` lies in `[start, end)`, an end of `None` reaching past every key.
fn contains(start: &[u8], end: Option<&[u8]>, key: &[u8]) -> bool {
    start <= key && end.is_none_or(|e| key < e)
}

/// Whether `[s1, e1)` and `[s2, e2)` share a key.
fn overlaps(s1: &[u8], e1: Option<&[u8]>, s2: &[u8], e2: Option<&[u8]>) -> bool {
    let start = s1.max(s2);
    match (e1, e2) {
        (None, None) => true,
        (Some(e), None) | (None, Some(e)) => start < e,
        (Some(a), Some(b)) => start < a.min(b),
    }
}

impl Run {
    /// The first point at or after `from` whose key is not below `key`.
    fn seek_point(&self, from: usize, key: &[u8]) -> usize {
        let rest = self.points.get(from..).unwrap_or(&[]);
        from + rest.partition_point(|p| p.key.of(&self.arena) < key)
    }

    /// Whether a version above `rv` wrote one of `keys`, whole or by a statement of its list. `keys` come in key
    /// order, so each one is sought in the rest of the run after the one before it: a merge join.
    pub fn keys_written<'k>(&self, keys: impl Iterator<Item = &'k [u8]>, rv: u64) -> bool {
        let mut at = 0;
        for key in keys {
            at = self.seek_point(at, key);
            if self
                .points
                .get(at)
                .is_some_and(|p| p.key.of(&self.arena) == key && p.newest() > rv)
            {
                return true;
            }
        }
        false
    }

    /// Whether a version above `rv` wrote one of `statements` or its list whole; in key and object order.
    pub fn statements_written<'k>(
        &self,
        statements: impl Iterator<Item = (&'k [u8], &'k [u8])>,
        rv: u64,
    ) -> bool {
        let (mut at, mut st) = (0, 0);
        for (key, object) in statements {
            at = self.seek_point(at, key);
            if self
                .points
                .get(at)
                .is_some_and(|p| p.key.of(&self.arena) == key && p.whole > rv)
            {
                return true;
            }
            let rest = self.statements.get(st..).unwrap_or(&[]);
            st += rest.partition_point(|s| {
                (s.key.of(&self.arena), s.object.of(&self.arena)) < (key, object)
            });
            if self.statements.get(st).is_some_and(|s| {
                s.key.of(&self.arena) == key && s.object.of(&self.arena) == object && s.version > rv
            }) {
                return true;
            }
        }
        false
    }

    /// Whether a version above `rv` wrote a key inside one of `ranges`, whole or by a statement of its list; in order
    /// of their starts.
    pub fn ranges_written<'k>(
        &self,
        ranges: impl Iterator<Item = (&'k [u8], Option<&'k [u8]>)>,
        rv: u64,
    ) -> bool {
        let mut at = 0;
        for (start, end) in ranges {
            at = self.seek_point(at, start);
            let inside = self
                .points
                .get(at..)
                .unwrap_or(&[])
                .iter()
                .take_while(|p| end.is_none_or(|e| p.key.of(&self.arena) < e));
            if inside.into_iter().any(|p| p.newest() > rv) {
                return true;
            }
        }
        false
    }

    /// Whether a range written above `rv` holds a key `rs` read: a list, a statement's list, an absent key, a key of
    /// a range or of the catalog range.
    pub fn ranges_over(&self, rs: &ReadSet, rv: u64) -> bool {
        let (cs, ce) = rs.catalog();
        self.ranges.iter().filter(|w| w.version > rv).any(|w| {
            let (ws, we) = (w.start.of(&self.arena), w.end.map(|e| e.of(&self.arena)));
            rs.lists().any(|k| contains(ws, we, k))
                || rs.statements().any(|(k, _)| contains(ws, we, k))
                || rs.absent().any(|k| contains(ws, we, k))
                || rs.ranges().any(|(s, e)| overlaps(ws, we, s, e))
                || overlaps(ws, we, cs, ce)
        })
    }
}
