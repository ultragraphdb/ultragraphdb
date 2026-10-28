/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The reference model of validation the conflict history and the resolver are tested against: every committed
//! transaction's writes kept whole in a list, each read checked against every write above its read version, pair by
//! pair, by the rules written out plainly. Slow and obviously right.

use ultragraphdb_protocol_wire::ConflictKind;
use ultragraphdb_transactions_txn::ReadSet;

use crate::utils::proposal_double::Writes;

/// Whether `key` lies in `[start, end)`, an end of `None` reaching past every key.
pub fn contains(start: &[u8], end: Option<&[u8]>, key: &[u8]) -> bool {
    start <= key && end.is_none_or(|e| key < e)
}

/// Whether `[s1, e1)` and `[s2, e2)` share a key.
pub fn overlaps(s1: &[u8], e1: Option<&[u8]>, s2: &[u8], e2: Option<&[u8]>) -> bool {
    let start = s1.max(s2);
    match (e1, e2) {
        (None, None) => true,
        (Some(e), None) | (None, Some(e)) => start < e,
        (Some(a), Some(b)) => start < a.min(b),
    }
}

/// Every commit, its version and its writes, and the floor.
#[derive(Debug, Clone, Default)]
pub struct Model {
    commits: Vec<(u64, Writes)>,
    floor: u64,
}

impl Model {
    pub fn new(floor: u64) -> Self {
        Self {
            commits: Vec::new(),
            floor,
        }
    }

    pub fn insert(&mut self, version: u64, writes: &Writes) {
        self.commits.push((version, writes.clone()));
    }

    pub fn prune(&mut self, floor: u64) {
        self.floor = self.floor.max(floor);
    }

    pub fn floor(&self) -> u64 {
        self.floor
    }

    /// The kind of the first read of `rs` (in the order list, statement, range, absent term, catalog) that a point or
    /// statement written above its read version conflicts with; else `Drop` when a written range does; else `None`.
    pub fn conflict(&self, rs: &ReadSet) -> Option<ConflictKind> {
        let rv = rs.read_version();
        let above: Vec<&Writes> = self
            .commits
            .iter()
            .filter(|(v, _)| *v > rv)
            .map(|(_, w)| w)
            .collect();
        let key_hit = |k: &[u8]| {
            above.iter().any(|w| {
                w.points.iter().any(|p| p.as_slice() == k)
                    || w.statements.iter().any(|(sk, _)| sk.as_slice() == k)
            })
        };
        let statement_hit = |k: &[u8], o: &[u8]| {
            above.iter().any(|w| {
                w.points.iter().any(|p| p.as_slice() == k)
                    || w.statements
                        .iter()
                        .any(|(sk, so)| sk.as_slice() == k && so.as_slice() == o)
            })
        };
        let range_hit = |s: &[u8], e: Option<&[u8]>| {
            above.iter().any(|w| {
                w.points.iter().any(|p| contains(s, e, p))
                    || w.statements.iter().any(|(sk, _)| contains(s, e, sk))
            })
        };
        if rs.lists().any(key_hit) {
            return Some(ConflictKind::List);
        }
        if rs.statements().any(|(k, o)| statement_hit(k, o)) {
            return Some(ConflictKind::Statement);
        }
        if rs.ranges().any(|(s, e)| range_hit(s, e)) {
            return Some(ConflictKind::Range);
        }
        if rs.absent().any(key_hit) {
            return Some(ConflictKind::AbsentTerm);
        }
        let (cs, ce) = rs.catalog();
        if range_hit(cs, ce) {
            return Some(ConflictKind::Catalog);
        }
        let dropped = above.iter().any(|w| {
            w.ranges.iter().any(|(ws, we)| {
                let we = we.as_deref();
                rs.lists().any(|k| contains(ws, we, k))
                    || rs.statements().any(|(k, _)| contains(ws, we, k))
                    || rs.absent().any(|k| contains(ws, we, k))
                    || rs.ranges().any(|(s, e)| overlaps(ws, we, s, e))
                    || overlaps(ws, we, cs, ce)
            })
        });
        dropped.then_some(ConflictKind::Drop)
    }
}
