/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_sim::{FaultConfig, Seed, StoreWorkload};

#[test]
fn test_the_workload_has_an_apply_loop_and_its_readers() {
    assert_eq!(StoreWorkload::new(3).actors(), 4);
    assert_eq!(StoreWorkload::new(0).actors(), 1);
}

#[test]
fn test_a_workload_without_readers_only_writes() {
    let outcome = StoreWorkload::run(Seed::new(5), FaultConfig::none(), 200, 0).unwrap();
    let steps: Vec<&String> = outcome
        .trace()
        .iter()
        .filter(|l| l.starts_with("step "))
        .collect();
    assert_eq!(steps.len(), 200);
    assert!(
        steps.iter().all(|l| l.ends_with(" picks 0")),
        "the apply loop is the only actor: {steps:?}"
    );
    for read in ["SnapshotOpened", "Read", "ReadMany", "Scanned"] {
        assert!(
            !outcome.trace().iter().any(|l| l.starts_with(read)),
            "{read} without a reader"
        );
    }
    assert!(outcome.trace().iter().any(|l| l.starts_with("Applied")));
}

/// Whether the trace line `line` is a record of the variant `kind`, and not of another whose name starts with it.
fn is_record(line: &str, kind: &str) -> bool {
    line == kind || line.starts_with(&format!("{kind} {{"))
}

#[test]
fn test_a_run_writes_reads_and_replays() {
    let first = StoreWorkload::run(Seed::new(11), FaultConfig::mixed(), 250, 2).unwrap();
    let again = StoreWorkload::run(Seed::new(11), FaultConfig::mixed(), 250, 2).unwrap();
    assert_eq!(first, again);
    let other = StoreWorkload::run(Seed::new(12), FaultConfig::mixed(), 250, 2).unwrap();
    assert_ne!(first.trace(), other.trace());
    let kinds = [
        "Applied",
        "Scanned",
        "Read",
        "ReadMany",
        "Dropped",
        "Compacted",
        "Swept",
        "Banned",
        "Refused",
        "RebuildRecorded",
        "TemporaryCreated",
        "TemporaryApplied",
        "TemporarySnapshotOpened",
        "TemporaryDropped",
    ];
    let all: Vec<String> = (0..6)
        .flat_map(|s| {
            StoreWorkload::run(Seed::new(s), FaultConfig::none(), 300, 2)
                .unwrap()
                .trace()
                .to_vec()
        })
        .collect();
    for kind in kinds {
        assert!(
            all.iter().any(|l| is_record(l, kind)),
            "no {kind} in six runs"
        );
    }
}

#[test]
fn test_mixed_faults_stop_rebuilds_and_fail_temporary_writes() {
    let all: Vec<String> = (0..24)
        .flat_map(|s| {
            StoreWorkload::run(Seed::new(s), FaultConfig::mixed(), 300, 2)
                .unwrap()
                .trace()
                .to_vec()
        })
        .collect();
    for kind in ["RebuildInterrupted", "TemporaryFailed"] {
        assert!(
            all.iter().any(|l| is_record(l, kind)),
            "no {kind} in 24 runs"
        );
    }
}

/// The read a trace line records without its answer, the snapshot's id first, and whether the answer failed.
fn read_of(line: &str) -> Option<(&str, bool)> {
    if !["Read {", "ReadMany {", "Scanned {"]
        .iter()
        .any(|kind| line.starts_with(kind))
    {
        return None;
    }
    let (read, answer) = line
        .split_once(", result: ")
        .or_else(|| line.split_once(", items: "))?;
    Some((read, answer.contains("Err(StoreError")))
}

/// A reader makes earlier reads again through its open snapshot, after other actors' steps, and makes a failed read
/// again at each of its later steps: what the repeat invariant compares.
#[test]
fn test_readers_make_earlier_reads_again_through_their_snapshots() {
    let trace = StoreWorkload::run(Seed::new(3), FaultConfig::none(), 300, 2)
        .unwrap()
        .trace()
        .to_vec();
    let reads: Vec<&str> = trace
        .iter()
        .filter_map(|l| read_of(l))
        .map(|(r, _)| r)
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    let again = reads.iter().filter(|r| !seen.insert(**r)).count();
    assert_eq!((reads.len(), again), (201, 56));

    let trace: Vec<String> = (0..8)
        .flat_map(|s| {
            StoreWorkload::run(Seed::new(s), FaultConfig::mixed(), 300, 2)
                .unwrap()
                .trace()
                .to_vec()
        })
        .collect();
    let reads: Vec<(&str, bool)> = trace.iter().filter_map(|l| read_of(l)).collect();
    let failed = reads.iter().filter(|(_, failed)| *failed).count();
    let retried = reads
        .iter()
        .enumerate()
        .filter(|(at, (read, failed))| *failed && reads[at + 1..].iter().any(|(r, _)| r == read))
        .count();
    assert_eq!((failed, retried), (28, 20));
}

/// A trace line with the versionless writes of the batch it records, if any, left out.
fn without_versionless(line: &str) -> String {
    match (line.find(", versionless: ["), line.find("], applied_ts: ")) {
        (Some(from), Some(to)) => format!("{}{}", &line[..from], &line[to + 1..]),
        _ => line.to_string(),
    }
}

/// Readers read keys without a version only when asked to, through their snapshots and through the store. Those
/// reads draw from the versionless stream and the fault stream alone, so without faults a run with them is the run
/// without them with its versionless reads added; only the versionless writes, drawn from the same stream, differ.
#[test]
fn test_versionless_reads_add_only_their_own_records() {
    let mut reads: Vec<String> = Vec::new();
    for seed in 0..4 {
        let without = StoreWorkload::run(Seed::new(seed), FaultConfig::none(), 300, 2).unwrap();
        let with = StoreWorkload::new(2)
            .with_versionless_reads()
            .run_on(Seed::new(seed), FaultConfig::none(), 300)
            .unwrap();
        let (versionless, rest): (Vec<&String>, Vec<&String>) = with
            .trace()
            .iter()
            .partition(|l| is_record(l, "ReadVersionless"));
        assert_eq!(
            rest.iter()
                .map(|l| without_versionless(l))
                .collect::<Vec<_>>(),
            without
                .trace()
                .iter()
                .map(|l| without_versionless(l))
                .collect::<Vec<_>>(),
            "seed {seed}"
        );
        assert_eq!(
            (with.contents().keys(), with.contents().applied_ts()),
            (without.contents().keys(), without.contents().applied_ts()),
            "seed {seed}"
        );
        assert!(
            !without
                .trace()
                .iter()
                .any(|l| is_record(l, "ReadVersionless")),
            "seed {seed} read keys without a version unasked"
        );
        reads.extend(versionless.into_iter().cloned());
    }
    for shape in ["ReadVersionless { id: Some(", "ReadVersionless { id: None,"] {
        assert!(reads.iter().any(|l| l.starts_with(shape)), "no {shape}");
    }
    for answer in ["result: Ok(Some(", "result: Ok(None)"] {
        assert!(reads.iter().any(|l| l.contains(answer)), "no {answer}");
    }
}

/// Commit batches write and remove keys without a version, laid out as the dictionary's: namespace 0, the IRI tag
/// 0x01, then an id below 8, each holding a u64 big-endian; 17 bytes of key and 8 of value, by that layout.
#[test]
fn test_commit_batches_write_and_remove_dictionary_shaped_versionless_keys() {
    let runs: Vec<_> = (0..4)
        .map(|seed| StoreWorkload::run(Seed::new(seed), FaultConfig::none(), 300, 2).unwrap())
        .collect();
    let held: Vec<(&Vec<u8>, &Vec<u8>)> = runs
        .iter()
        .flat_map(|r| r.contents().versionless().iter())
        .collect();
    assert!(!held.is_empty(), "no key without a version was left");
    for (key, value) in held {
        assert_eq!(key.len(), 17, "{key:?}");
        assert_eq!(key[..9], [0, 0, 0, 0, 0, 0, 0, 0, 0x01], "{key:?}");
        assert!(key[9..16].iter().all(|&b| b == 0) && key[16] < 8, "{key:?}");
        assert_eq!(value.len(), 8, "{value:?}");
    }
    let applied: Vec<&String> = runs
        .iter()
        .flat_map(|r| r.trace())
        .filter(|l| is_record(l, "Applied"))
        .collect();
    assert!(
        applied
            .iter()
            .any(|l| l.contains("versionless: [VersionlessEntry {")),
        "no batch wrote a key without a version"
    );
    assert!(
        applied.iter().any(|l| l.contains("value: None }")),
        "no batch removed a key without a version"
    );
}

/// The bytes of the Debug list `[1, 2, 3]` that starts right after `marker` in `line`.
fn bytes_after(line: &str, marker: &str) -> Option<Vec<u8>> {
    let rest = &line[line.find(marker)? + marker.len()..];
    let list = &rest[..rest.find(']')?];
    list.split(", ")
        .filter(|b| !b.is_empty())
        .map(|b| b.parse().ok())
        .collect()
}

/// The entries a trace line's batch writes: each key, version and value, `None` for a deletion.
fn entries_of(line: &str) -> Vec<(Vec<u8>, u64, Option<Vec<u8>>)> {
    line.split("Entry { key: ")
        .skip(1)
        .filter_map(|entry| {
            let key = bytes_after(entry, "[")?;
            let version = entry
                .split("version: ")
                .nth(1)?
                .split(',')
                .next()?
                .parse()
                .ok()?;
            let value = if entry.contains("value: None") {
                None
            } else {
                Some(bytes_after(entry, "value: Some([")?)
            };
            Some((key, version, value))
        })
        .collect()
}

/// A drop of a namespace's catalog writes the declaration of predicate 7 there, and a drop of one predicate's
/// catalog entry writes that entry again, at version 1 with meta 1 and the value `name`; by the catalog layout
/// `[0x01][namespace u64][0x01][predicate u64]`, the namespace prefix is 9 bytes and an entry 18.
#[test]
fn test_a_catalog_drop_writes_its_declaration_under_its_prefix() {
    let mut seen = std::collections::BTreeSet::new();
    for seed in 0..8 {
        let run = StoreWorkload::run(Seed::new(seed), FaultConfig::none(), 300, 0).unwrap();
        for line in run.trace().iter().filter(|l| is_record(l, "Dropped")) {
            let Some(prefix) = bytes_after(line, "prefixes: Some([[") else {
                continue;
            };
            let entries = entries_of(line);
            if prefix[0] != 0x01 {
                assert!(entries.is_empty(), "{line}");
                continue;
            }
            let mut declaration = prefix.clone();
            if prefix.len() == 9 {
                declaration.push(0x01);
                declaration.extend_from_slice(&7u64.to_be_bytes());
            }
            assert!(prefix.len() == 9 || prefix.len() == 18, "{line}");
            match entries.as_slice() {
                [] => {}
                [(key, 1, Some(value))] => {
                    assert_eq!(
                        (key, value.as_slice()),
                        (&declaration, &b"name"[..]),
                        "{line}"
                    );
                    assert!(line.contains("meta: 1,"), "{line}");
                    // The prefix's length, whether its namespace is other than 0, and its predicate byte.
                    seen.insert((prefix.len(), prefix[8] != 0, prefix[prefix.len() - 1]));
                }
                other => panic!("{other:?} in {line}"),
            }
        }
    }
    for case in [(9, true), (18, true)] {
        assert!(
            seen.iter().any(|(len, ns, _)| (*len, *ns) == case),
            "no {case:?} in {seen:?}"
        );
    }
    assert!(
        seen.iter()
            .any(|(len, _, predicate)| *len == 18 && *predicate == 9),
        "no drop of predicate 9's entry in {seen:?}"
    );
}

/// A rollup writes a complete version above its key's newest deletion, so it never replaces a deletion and never
/// lands at version 0; without faults, the deletions are the workload's own earlier writes in the trace.
#[test]
fn test_a_rollup_lands_above_the_newest_deletion_of_its_key() {
    let mut rollups = 0;
    for seed in 0..8 {
        let run = StoreWorkload::run(Seed::new(seed), FaultConfig::none(), 300, 0).unwrap();
        let mut written: std::collections::BTreeMap<Vec<u8>, u64> = Default::default();
        for line in run.trace().iter().filter(|l| is_record(l, "Applied")) {
            for (key, version, value) in entries_of(line) {
                let deleted = written.entry(key).or_default();
                if value.as_deref() == Some(b"rolled up") {
                    assert!(
                        *deleted < version,
                        "a rollup at {version} on a deletion at {deleted}: {line}"
                    );
                    rollups += 1;
                }
                if value.is_none() {
                    *deleted = (*deleted).max(version);
                }
            }
        }
    }
    assert!(rollups > 0, "no rollup in eight runs");
}

/// The workload bans namespaces 1 and 5, its two besides the root, and never the root namespace 0.
#[test]
fn test_the_workload_never_bans_the_root_namespace() {
    let banned: std::collections::BTreeSet<String> = (0..8)
        .flat_map(|seed| {
            StoreWorkload::run(Seed::new(seed), FaultConfig::none(), 300, 0)
                .unwrap()
                .trace()
                .to_vec()
        })
        .filter(|l| is_record(l, "Banned"))
        .collect();
    assert_eq!(
        banned,
        ["Banned { namespace: 1 }", "Banned { namespace: 5 }"]
            .into_iter()
            .map(String::from)
            .collect()
    );
}
