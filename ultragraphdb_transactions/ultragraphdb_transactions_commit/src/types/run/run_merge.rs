/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cmp::Ordering;

use crate::types::run::{Point, Run, Statement, Written};

/// A statement's list key and object in `arena`.
fn pair<'a>(s: &Statement, arena: &'a [u8]) -> (&'a [u8], &'a [u8]) {
    (s.key.of(arena), s.object.of(arena))
}

/// A version kept above `floor`, 0 when it is at or below it.
fn above(version: u64, floor: u64) -> u64 {
    if version > floor { version } else { 0 }
}

impl Run {
    /// Fills the run, cleared, with `a` and `b` merged: each key and statement once with the newest version of the
    /// two, the ranges of both, and nothing at or below `floor`.
    pub fn merge(&mut self, a: &Run, b: &Run, floor: u64) {
        self.clear();
        let (mut i, mut j) = (0, 0);
        loop {
            let (point, arena) = match (a.points.get(i), b.points.get(j)) {
                (None, None) => break,
                (Some(p), None) => {
                    i += 1;
                    (*p, a.arena.as_slice())
                }
                (None, Some(q)) => {
                    j += 1;
                    (*q, b.arena.as_slice())
                }
                (Some(p), Some(q)) => match p.key.of(&a.arena).cmp(q.key.of(&b.arena)) {
                    Ordering::Less => {
                        i += 1;
                        (*p, a.arena.as_slice())
                    }
                    Ordering::Greater => {
                        j += 1;
                        (*q, b.arena.as_slice())
                    }
                    Ordering::Equal => {
                        i += 1;
                        j += 1;
                        let both = Point {
                            key: p.key,
                            whole: p.whole.max(q.whole),
                            touched: p.touched.max(q.touched),
                        };
                        (both, a.arena.as_slice())
                    }
                },
            };
            let (whole, touched) = (above(point.whole, floor), above(point.touched, floor));
            if whole == 0 && touched == 0 {
                continue;
            }
            let key = self.add(point.key.of(arena));
            self.points.push(Point {
                key,
                whole,
                touched,
            });
            self.newest = self.newest.max(whole).max(touched);
        }
        let (mut i, mut j) = (0, 0);
        loop {
            let (statement, arena) = match (a.statements.get(i), b.statements.get(j)) {
                (None, None) => break,
                (Some(s), None) => {
                    i += 1;
                    (*s, a.arena.as_slice())
                }
                (None, Some(t)) => {
                    j += 1;
                    (*t, b.arena.as_slice())
                }
                (Some(s), Some(t)) => match pair(s, &a.arena).cmp(&pair(t, &b.arena)) {
                    Ordering::Less => {
                        i += 1;
                        (*s, a.arena.as_slice())
                    }
                    Ordering::Greater => {
                        j += 1;
                        (*t, b.arena.as_slice())
                    }
                    Ordering::Equal => {
                        i += 1;
                        j += 1;
                        let newest = Statement {
                            version: s.version.max(t.version),
                            ..*s
                        };
                        (newest, a.arena.as_slice())
                    }
                },
            };
            if statement.version <= floor {
                continue;
            }
            let key = self.add(statement.key.of(arena));
            let object = self.add(statement.object.of(arena));
            self.statements.push(Statement {
                key,
                object,
                version: statement.version,
            });
            self.newest = self.newest.max(statement.version);
        }
        for (run, range) in a
            .ranges
            .iter()
            .map(|r| (a, r))
            .chain(b.ranges.iter().map(|r| (b, r)))
        {
            if range.version <= floor {
                continue;
            }
            let start = self.add(range.start.of(&run.arena));
            let end = range.end.map(|e| self.add(e.of(&run.arena)));
            self.ranges.push(Written {
                start,
                end,
                version: range.version,
            });
            self.newest = self.newest.max(range.version);
        }
    }
}
