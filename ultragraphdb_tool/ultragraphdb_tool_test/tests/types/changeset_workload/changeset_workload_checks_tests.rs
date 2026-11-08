/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The changeset workload's checks against a store that breaks a rule: a store some keys were deleted from, or written to,
//! behind the applier's back, as no apply does, is reported naming the rule the damage breaks.

use ultragraphdb_core_ids::{Sequence, SystemId};
use ultragraphdb_core_keys::{DEFAULT_GRAPH, Keyspace, QuadKey, ROOT_TENANT, key_prefix};
use ultragraphdb_store_sim::{FaultConfig, Invariant, Seed, SimStore, Simulation, Violation};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_test::{
    ChangesetWorkload, INDEXES_MATCH_DATA, NO_UNEXPECTED_ERROR, RECORDS_MATCH_POSTINGS,
    REPLAY_AGREES, STATISTICS_MATCH_DATA,
};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

/// Steps before the damage: enough for writes to hold data in every keyspace the checks read.
const BEFORE: u64 = 40;

/// Steps after it: enough for a write to commit above the damage, which the checks then read.
const AFTER: u64 = 20;

/// Deletes, at a version of its own above every commit, each key of `keyspace` whose newest version holds a value.
fn delete_keyspace(store: &SimStore, keyspace: Keyspace) -> Result<(), String> {
    let at = store.applied_ts() + 1;
    let mut batch = WriteBatch::new();
    for (key, versions) in store.contents().keys() {
        let holds = versions.first().is_some_and(|v| !v.is_deletion());
        let ours = key
            .first()
            .and_then(|b| Keyspace::from_byte(*b))
            .is_some_and(|(k, split)| k == keyspace && !split);
        if holds && ours {
            batch.push(Entry::deletion(key.clone(), at));
        }
    }
    batch.set_applied_ts(at);
    store
        .apply(batch, PersistMode::Buffer)
        .map_err(|e| e.to_string())
}

/// Writes `entries` in a batch of their own at a version above every commit.
fn written(store: &SimStore, entries: Vec<(Vec<u8>, Vec<u8>)>) -> Result<(), String> {
    let at = store.applied_ts() + 1;
    let mut batch = WriteBatch::new();
    for (key, value) in entries {
        batch.push(Entry::put(key, at, 0, value));
    }
    batch.set_applied_ts(at);
    store
        .apply(batch, PersistMode::Buffer)
        .map_err(|e| e.to_string())
}

/// The newest value of each key of `keyspace` the store holds one for.
fn held(store: &SimStore, keyspace: Keyspace) -> Vec<(Vec<u8>, Vec<u8>)> {
    store
        .contents()
        .keys()
        .iter()
        .filter(|(key, _)| {
            key.first()
                .and_then(|b| Keyspace::from_byte(*b))
                .is_some_and(|(k, split)| k == keyspace && !split)
        })
        .filter_map(|(key, versions)| {
            let newest = versions.first().filter(|v| !v.is_deletion())?;
            Some((key.clone(), newest.value().to_vec()))
        })
        .collect()
}

/// The rule the changeset workload reports when `keyspace`'s keys are deleted after `BEFORE` steps of `seed`.
fn reported(seed: u64, keyspace: Keyspace) -> TestResult<Violation> {
    damaged(seed, |store| delete_keyspace(store, keyspace))
}

/// The rule the changeset workload reports when `damage` is done to its store after `BEFORE` steps of `seed`.
fn damaged(seed: u64, damage: impl Fn(&SimStore) -> Result<(), String>) -> TestResult<Violation> {
    damaged_in(ChangesetWorkload::default(), seed, damage)
}

/// The rule `workload` reports when `damage` is done to its store after `BEFORE` steps of `seed`.
fn damaged_in(
    mut workload: ChangesetWorkload,
    seed: u64,
    damage: impl Fn(&SimStore) -> Result<(), String>,
) -> TestResult<Violation> {
    let mut taken = 0;
    let run = Simulation::new(Seed::new(seed), FaultConfig::none()).run(
        BEFORE + AFTER,
        2,
        |ctx, event| {
            if taken == BEFORE {
                damage(ctx.store())
                    .map_err(|detail| ctx.violation("the damage is written", detail))?;
            }
            taken += 1;
            workload.step(ctx, event)
        },
    );
    run.err().context("the damage is reported")
}

#[test]
fn test_deleted_statistics_break_the_statistics_rule() -> TestResult {
    let violation = reported(1, Keyspace::Statistics)?;
    assert_eq!(
        violation.invariant(),
        Invariant::Workload(STATISTICS_MATCH_DATA),
        "{}",
        violation.detail()
    );
    Ok(())
}

#[test]
fn test_deleted_value_index_keys_break_the_index_rule() -> TestResult {
    let violation = reported(1, Keyspace::ValueIndex)?;
    assert_eq!(
        violation.invariant(),
        Invariant::Workload(INDEXES_MATCH_DATA),
        "{}",
        violation.detail()
    );
    Ok(())
}

/// The rule a damage breaks: each named by the workload's check of it.
fn assert_breaks(violation: &Violation, rule: &'static str) {
    assert_eq!(
        violation.invariant(),
        Invariant::Workload(rule),
        "{}",
        violation.detail()
    );
}

#[test]
fn test_deleted_predicate_lists_and_counts_break_the_index_rule() -> TestResult {
    for keyspace in [
        Keyspace::SubjectPredicates,
        Keyspace::ObjectPredicates,
        Keyspace::CountIndex,
        Keyspace::Reverse,
    ] {
        assert_breaks(&reported(1, keyspace)?, INDEXES_MATCH_DATA);
    }
    Ok(())
}

#[test]
fn test_deleted_statement_records_break_the_records_rule() -> TestResult {
    assert_breaks(&reported(1, Keyspace::Statements)?, RECORDS_MATCH_POSTINGS);
    Ok(())
}

/// A key of the statistics keyspace that is no statistics key is an index the data does not explain.
#[test]
fn test_a_statistics_key_that_does_not_parse_breaks_the_statistics_rule() -> TestResult {
    let violation = damaged(1, |store| {
        // Under the default graph's prefix, where the check reads, and too short for a statistics key.
        let mut key = key_prefix(
            Keyspace::Statistics,
            false,
            ROOT_TENANT,
            Some(DEFAULT_GRAPH),
            None,
        );
        key.push(0xFF);
        written(store, vec![(key, vec![0])])
    })?;
    assert_breaks(&violation, STATISTICS_MATCH_DATA);
    Ok(())
}

/// A statement record copied under an ID no posting carries is an orphan.
#[test]
fn test_a_record_no_posting_carries_breaks_the_records_rule() -> TestResult {
    let violation = damaged(1, |store| {
        let records = held(store, Keyspace::Statements);
        let (key, value) = records.first().ok_or("a statement record")?;
        let Ok((
            QuadKey::Statement {
                tenant,
                graph,
                statement,
            },
            _,
        )) = QuadKey::parse(key)
        else {
            return Err("no statement key".to_string());
        };
        let sequence =
            Sequence::new(statement.sequence().get() + 1_000_000).map_err(|e| e.to_string())?;
        let orphan = QuadKey::Statement {
            tenant,
            graph,
            statement: SystemId::new(statement.type_prefix(), statement.bucket(), sequence),
        };
        written(store, vec![(orphan.to_bytes(), value.clone())])
    })?;
    assert_breaks(&violation, RECORDS_MATCH_POSTINGS);
    assert!(
        violation.detail().contains("has no posting"),
        "{}",
        violation.detail()
    );
    Ok(())
}

/// A forward list whose bytes are no posting list fails the check with an error no injected fault explains.
#[test]
fn test_a_forward_list_that_does_not_decode_is_an_unexpected_error() -> TestResult {
    let violation = damaged(1, |store| {
        let lists = held(store, Keyspace::Forward);
        let (key, _) = lists.first().ok_or("a forward list")?;
        written(store, vec![(key.clone(), vec![0xFF, 0, 0])])
    })?;
    assert_breaks(&violation, NO_UNEXPECTED_ERROR);
    Ok(())
}

/// A replayed run whose store lost keys the replica holds reports the replay's rule, naming the keys.
#[test]
fn test_a_store_differing_from_its_replica_in_a_key_breaks_the_replay_rule() -> TestResult {
    let violation = damaged_in(ChangesetWorkload::replaying(), 1, |store| {
        delete_keyspace(store, Keyspace::Statistics)
    })?;
    assert_breaks(&violation, REPLAY_AGREES);
    assert!(
        violation.detail().starts_with("keys "),
        "{}",
        violation.detail()
    );
    Ok(())
}

/// A replayed run whose store holds a key without versions the replica does not reports the replay's rule, naming it.
#[test]
fn test_a_store_differing_from_its_replica_in_a_key_without_versions_breaks_the_replay_rule()
-> TestResult {
    let violation = damaged_in(ChangesetWorkload::replaying(), 1, |store| {
        let mut batch = WriteBatch::new();
        batch.put_versionless(b"\xffdamage".to_vec(), vec![1]);
        store
            .apply(batch, PersistMode::Buffer)
            .map_err(|e| e.to_string())
    })?;
    assert_breaks(&violation, REPLAY_AGREES);
    assert!(
        violation.detail().starts_with("keys without versions"),
        "{}",
        violation.detail()
    );
    Ok(())
}

/// A replayed run whose store's metadata alone differs from the replica's, its discard timestamp, reports the replay's
/// rule naming both.
#[test]
fn test_a_store_differing_from_its_replica_in_its_metadata_breaks_the_replay_rule() -> TestResult {
    let violation = damaged_in(ChangesetWorkload::replaying(), 1, |store| {
        store.set_discard_ts(1).map_err(|e| e.to_string())
    })?;
    assert_breaks(&violation, REPLAY_AGREES);
    assert!(
        violation.detail().starts_with("metadata"),
        "{}",
        violation.detail()
    );
    Ok(())
}

/// A statement's record overwritten with another statement's names a statement other than the posting carrying its ID.
#[test]
fn test_a_record_naming_another_statement_breaks_the_records_rule() -> TestResult {
    let violation = damaged(1, |store| {
        let records = held(store, Keyspace::Statements);
        let mut differing = records
            .iter()
            .filter(|(_, value)| records.first().is_some_and(|(_, first)| first != value));
        let ((first, _), (_, other)) = records
            .first()
            .zip(differing.next())
            .ok_or("two records of different statements")?;
        written(store, vec![(first.clone(), other.clone())])
    })?;
    assert_breaks(&violation, RECORDS_MATCH_POSTINGS);
    assert!(
        violation.detail().contains("names another statement"),
        "{}",
        violation.detail()
    );
    Ok(())
}
