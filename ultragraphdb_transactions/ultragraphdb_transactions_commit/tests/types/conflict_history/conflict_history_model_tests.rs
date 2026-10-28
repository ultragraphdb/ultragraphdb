/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The conflict history and the resolver against the reference model, over random histories: commits at rising
//! versions, prunes at random floors, and checks at random read versions around the floor, keys drawn from a small
//! alphabet so they repeat, share prefixes and fall inside each other's ranges.

use ultragraphdb_protocol_wire::WriteSetRecordView;
use ultragraphdb_transactions_commit::{CommitError, ConflictHistory, Resolver};

use crate::utils::fixture::Rng;
use crate::utils::model::Model;
use crate::utils::proposal_double::{Reads, Writes, read_set, record};

fn reads(rng: &mut Rng) -> Reads {
    let mut reads = Reads::default();
    for _ in 0..rng.below(4) {
        reads.lists.push(rng.key(3));
    }
    for _ in 0..rng.below(3) {
        reads.statements.push((rng.key(2), rng.key(2)));
    }
    for _ in 0..rng.below(2) {
        let start = rng.key(2);
        let end = (rng.below(4) != 0).then(|| rng.key(3));
        reads.ranges.push((start, end));
    }
    for _ in 0..rng.below(2) {
        reads.absent.push(rng.key(3));
    }
    reads
}

fn writes(rng: &mut Rng) -> Writes {
    let mut writes = Writes::default();
    for _ in 0..rng.below(4) {
        writes.points.push(rng.key(3));
    }
    for _ in 0..rng.below(4) {
        writes.statements.push((rng.key(2), rng.key(2)));
    }
    if rng.below(8) == 0 {
        let start = rng.key(2);
        let end = (rng.below(3) != 0).then(|| rng.key(3));
        writes.ranges.push((start, end));
    }
    writes
}

fn insert(history: &mut ConflictHistory, version: u64, writes: &Writes) {
    history.insert(
        version,
        writes.points.iter().map(Vec::as_slice),
        writes
            .statements
            .iter()
            .map(|(k, o)| (k.as_slice(), o.as_slice())),
        writes
            .ranges
            .iter()
            .map(|(s, e)| (s.as_slice(), e.as_deref())),
    );
}

#[test]
fn test_the_history_agrees_with_the_model_over_random_histories() {
    let mut checks = 0usize;
    let mut conflicts = 0usize;
    for seed in 0..40u64 {
        let mut rng = Rng::new(seed);
        let mut history = ConflictHistory::new(0);
        let mut model = Model::new(0);
        let mut version = 0u64;
        for _ in 0..400 {
            match rng.below(10) {
                0..=4 => {
                    version += 2;
                    let w = writes(&mut rng);
                    insert(&mut history, version, &w);
                    model.insert(version, &w);
                }
                5 => {
                    let floor = history.floor() + rng.below(version - history.floor() + 1);
                    history.prune(floor);
                    model.prune(floor);
                    assert_eq!(history.floor(), model.floor());
                }
                _ => {
                    // A read version at the floor or above it, odd ones included.
                    let rv = model.floor() + rng.below(version.saturating_sub(model.floor()) + 2);
                    let rs = read_set(0, rv, &reads(&mut rng));
                    let expected = model.conflict(&rs);
                    assert_eq!(history.conflict(&rs), expected, "seed {seed}, rv {rv}");
                    checks += 1;
                    conflicts += usize::from(expected.is_some());
                }
            }
        }
    }
    // The run exercised both outcomes.
    assert!(checks > 5_000, "{checks}");
    assert!(
        conflicts > checks / 10 && conflicts < checks * 9 / 10,
        "{conflicts} of {checks}"
    );
}

#[test]
fn test_the_resolver_agrees_with_the_model_on_too_old_and_conflicts() {
    let mut too_old = 0usize;
    for seed in 100..130u64 {
        let mut rng = Rng::new(seed);
        let mut resolver = Resolver::new(0);
        let mut model = Model::new(0);
        let mut version = 0u64;
        for _ in 0..300 {
            match rng.below(8) {
                0..=3 => {
                    version += 2;
                    let w = writes(&mut rng);
                    let bytes = record(0, 0, &w, 0);
                    let view = WriteSetRecordView::new(&bytes).expect("the record reads");
                    resolver.admit(version, &view);
                    model.insert(version, &w);
                }
                4 => {
                    let floor = version.saturating_sub(rng.below(12));
                    resolver.prune(floor);
                    model.prune(floor);
                }
                _ => {
                    // Read versions straddling the floor: the one just below it is too old.
                    let floor = model.floor();
                    let rv = floor.saturating_sub(2) + rng.below(version.saturating_sub(floor) + 4);
                    let rs = read_set(0, rv, &reads(&mut rng));
                    let expected = if rv < floor {
                        too_old += 1;
                        Err(CommitError::TooOld(rv, floor))
                    } else {
                        model
                            .conflict(&rs)
                            .map_or(Ok(()), |k| Err(CommitError::Conflict(k)))
                    };
                    assert_eq!(resolver.check(&rs), expected, "seed {seed}");
                }
            }
        }
    }
    assert!(too_old > 100, "{too_old}");
}
