/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Measures the design choice behind `ConflictHistory`: sorted runs whose keys sit in reused arenas, merged as a
//! binary counter and pruned whole, against the design's first sketch, a `BTreeMap` of owned keys to versions (and one
//! of owned statements) pruned through a log of what each transaction wrote. Each transaction writes 8 forward-list
//! keys of 25 bytes and 4 statements, and reads 16 lists and 4 statements, its subjects drawn from a million so that
//! conflicts are rare. Two costs are measured over a history holding the last `window` transactions: admitting the
//! next transaction (insert and prune, the steady state), and checking a read set, recent (four transactions above its
//! read version) and old (at the floor, the whole window above it). It does not measure the execution or the round's
//! body.

use std::collections::{BTreeMap, VecDeque};
use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use ultragraphdb_transactions_commit::ConflictHistory;
use ultragraphdb_transactions_txn::ReadSet;

/// A forward-list key of 25 bytes for `subject`.
fn key(subject: u64) -> Vec<u8> {
    let mut key = vec![0x10];
    key.extend_from_slice(&0u64.to_be_bytes());
    key.extend_from_slice(&7u64.to_be_bytes());
    key.extend_from_slice(&subject.to_be_bytes());
    key
}

/// A linear congruential generator; a fixed seed replays a run.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        (self.0 >> 16) % 1_000_000
    }
}

/// What one transaction wrote.
struct Writes {
    points: Vec<Vec<u8>>,
    statements: Vec<(Vec<u8>, Vec<u8>)>,
}

fn writes(rng: &mut Lcg) -> Writes {
    Writes {
        points: (0..8).map(|_| key(rng.next())).collect(),
        statements: (0..4)
            .map(|_| (key(rng.next()), rng.next().to_be_bytes()[..].to_vec()))
            .collect(),
    }
}

fn read_set(rng: &mut Lcg, read_version: u64) -> ReadSet {
    let mut rs = ReadSet::new();
    rs.reset(0, read_version, usize::MAX);
    for _ in 0..16 {
        rs.push_list(&key(rng.next())).expect("within the limit");
    }
    for _ in 0..4 {
        rs.push_statement(&key(rng.next()), &rng.next().to_be_bytes())
            .expect("within the limit");
    }
    rs.seal();
    rs
}

/// The design's first sketch: owned keys in ordered maps, pruned through a log of each transaction's keys.
#[derive(Default)]
struct BTreeHistory {
    points: BTreeMap<Vec<u8>, u64>,
    statements: BTreeMap<(Vec<u8>, Vec<u8>), u64>,
    log: VecDeque<(u64, Writes)>,
}

impl BTreeHistory {
    fn insert(&mut self, version: u64, w: &Writes) {
        for k in &w.points {
            self.points.insert(k.clone(), version);
        }
        for (k, o) in &w.statements {
            self.statements.insert((k.clone(), o.clone()), version);
        }
        let copy = Writes {
            points: w.points.clone(),
            statements: w.statements.clone(),
        };
        self.log.push_back((version, copy));
    }

    fn prune(&mut self, floor: u64) {
        while self.log.front().is_some_and(|(v, _)| *v <= floor) {
            if let Some((_, w)) = self.log.pop_front() {
                for k in &w.points {
                    if self.points.get(k).is_some_and(|v| *v <= floor) {
                        self.points.remove(k);
                    }
                }
                for s in w.statements {
                    if self.statements.get(&s).is_some_and(|v| *v <= floor) {
                        self.statements.remove(&s);
                    }
                }
            }
        }
    }

    fn conflicts(&self, rs: &ReadSet) -> bool {
        let rv = rs.read_version();
        let list = |k: &[u8]| {
            self.points.get(k).is_some_and(|v| *v > rv)
                || self
                    .statements
                    .range((k.to_vec(), Vec::new())..)
                    .take_while(|((sk, _), _)| sk.as_slice() == k)
                    .any(|(_, v)| *v > rv)
        };
        rs.lists().any(list)
            || rs.statements().any(|(k, o)| {
                self.points.get(k).is_some_and(|v| *v > rv)
                    || self
                        .statements
                        .get(&(k.to_vec(), o.to_vec()))
                        .is_some_and(|v| *v > rv)
            })
    }
}

fn insert_runs(h: &mut ConflictHistory, version: u64, w: &Writes) {
    h.insert(
        version,
        w.points.iter().map(Vec::as_slice),
        w.statements
            .iter()
            .map(|(k, o)| (k.as_slice(), o.as_slice())),
        std::iter::empty(),
    );
}

fn history(c: &mut Criterion) {
    let mut admit = c.benchmark_group("history_admit");
    for window in [1_000u64, 10_000] {
        let mut rng = Lcg(1);
        let pool: Vec<Writes> = (0..4_096).map(|_| writes(&mut rng)).collect();
        let mut runs = ConflictHistory::new(0);
        let mut tree = BTreeHistory::default();
        for (i, w) in pool.iter().cycle().take(window as usize).enumerate() {
            insert_runs(&mut runs, 2 * (i as u64 + 1), w);
            tree.insert(2 * (i as u64 + 1), w);
        }
        let mut version = 2 * window;
        let mut at = 0usize;
        admit.bench_with_input(BenchmarkId::new("runs", window), &window, |b, window| {
            b.iter(|| {
                version += 2;
                at = (at + 1) % pool.len();
                insert_runs(&mut runs, version, &pool[at]);
                runs.prune(version - 2 * window);
                black_box(runs.runs())
            })
        });
        let mut version = 2 * window;
        let mut at = 0usize;
        admit.bench_with_input(BenchmarkId::new("btree", window), &window, |b, window| {
            b.iter(|| {
                version += 2;
                at = (at + 1) % pool.len();
                tree.insert(version, &pool[at]);
                tree.prune(version - 2 * window);
                black_box(tree.log.len())
            })
        });
    }
    admit.finish();

    let mut check = c.benchmark_group("history_check");
    for window in [1_000u64, 10_000] {
        let mut rng = Lcg(2);
        let mut runs = ConflictHistory::new(0);
        let mut tree = BTreeHistory::default();
        for i in 0..window {
            let w = writes(&mut rng);
            insert_runs(&mut runs, 2 * (i + 1), &w);
            tree.insert(2 * (i + 1), &w);
        }
        for (name, rv) in [("recent", 2 * window - 8), ("old", 0)] {
            let sets: Vec<ReadSet> = (0..256).map(|_| read_set(&mut rng, rv)).collect();
            let mut at = 0usize;
            check.bench_function(BenchmarkId::new(format!("runs_{name}"), window), |b| {
                b.iter(|| {
                    at = (at + 1) % sets.len();
                    black_box(runs.conflict(&sets[at]))
                })
            });
            check.bench_function(BenchmarkId::new(format!("btree_{name}"), window), |b| {
                b.iter(|| {
                    at = (at + 1) % sets.len();
                    black_box(tree.conflicts(&sets[at]))
                })
            });
        }
    }
    check.finish();
}

criterion_group!(benches, history);
criterion_main!(benches);
