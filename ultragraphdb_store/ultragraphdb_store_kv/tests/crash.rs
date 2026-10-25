/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Kills a process writing into a fjall store, during a batch and at each stage of a drop, and reopens the store: a
//! batch is present whole or absent whole, the batches present are a prefix of batch order with the applied timestamp
//! of the last, and a drop whose record was synced is finished at open. A drop, a temporary keyspace and a rebuild
//! record are killed at the stages the writer stops at, never on a timer.

mod crash_harness;

use std::time::Duration;

use ultragraphdb_traits_store::Store;

use crate::crash_harness::{
    DROP_STAGES, DROPPED_KEYS, KEYS_PER_BATCH, RECORD_STAGES, TEMPORARY_STAGES, batch_counts,
    drop_outcome, fresh_dir, kill_after, kill_at, open, raw_drop_state, raw_temporary_keyspaces,
};

#[test]
fn test_a_kill_during_a_batch_leaves_it_whole_or_absent() {
    let mut in_flight = 0;
    for (round, delay) in [0u64, 1, 2, 3, 5, 8].into_iter().enumerate() {
        let dir = fresh_dir(&format!("batch-{round}"));
        let lines = kill_after(&dir, "batches", "batch 3", Duration::from_millis(delay));
        let started = lines.iter().filter(|l| l.starts_with("batch ")).count() as u64;
        let applied = lines.iter().filter(|l| l.starts_with("applied ")).count() as u64;
        if started > applied {
            in_flight += 1;
        }
        let store = open(&dir);
        let counts = batch_counts(&store, started + 1);
        assert!(
            counts.iter().all(|&c| c == 0 || c == KEYS_PER_BATCH),
            "round {round}: a batch is visible in part: {counts:?}"
        );
        let present = counts.iter().take_while(|&&c| c == KEYS_PER_BATCH).count() as u64;
        assert!(
            counts[present as usize..].iter().all(|&c| c == 0),
            "round {round}: not a prefix: {counts:?}"
        );
        assert!(
            present >= applied,
            "round {round}: an applied batch was lost: {counts:?}, {applied} applied"
        );
        assert_eq!(
            store.applied_ts(),
            present,
            "round {round}: the applied timestamp names the last batch held"
        );
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }
    println!("{in_flight} of 6 kills landed while a batch was written");
}

/// The writer announces every stage of its drop, temporary and record modes, in order, when it runs to the last.
#[test]
fn test_the_writer_announces_each_stage_it_reaches() {
    for (mode, stages) in [
        ("drop", DROP_STAGES.as_slice()),
        ("temporary", TEMPORARY_STAGES.as_slice()),
        ("record", RECORD_STAGES.as_slice()),
    ] {
        let dir = fresh_dir(&format!("stages-{mode}"));
        let last = stages.last().expect("a mode has stages");
        let announced: Vec<String> = kill_at(&dir, mode, last)
            .into_iter()
            .filter(|l| l.starts_with("stage "))
            .collect();
        let expected: Vec<String> = stages.iter().map(|s| format!("stage {s}")).collect();
        assert_eq!(announced, expected, "{mode}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A drop killed at each of its stages: before its record nothing is dropped; from the synced record on, the open
/// finishes it, removals interrupted partway included.
#[test]
fn test_a_kill_during_a_drop_is_finished_at_open() {
    let mut interrupted = 0;
    for stage in DROP_STAGES {
        let dir = fresh_dir(&format!("drop-{stage}"));
        kill_at(&dir, "drop", stage);
        let (record, left) = raw_drop_state(&dir);
        println!("drop killed at {stage}: record {record}, {left} keys left");
        let expected_record = !matches!(stage, "start" | "dropped");
        assert_eq!(record, expected_record, "{stage}: the drop record");
        let expected_left = match stage {
            "start" | "record-synced" => DROPPED_KEYS,
            "data-removed" => 0,
            "entries-synced" | "dropped" => 1,
            removing => {
                DROPPED_KEYS
                    - removing
                        .trim_start_matches("removing-")
                        .parse::<usize>()
                        .expect("a count")
            }
        };
        assert_eq!(left, expected_left, "{stage}: keys under the prefix");
        if stage.starts_with("removing-") && left > 0 {
            interrupted += 1;
        }
        let store = open(&dir);
        let (under, entry, kept) = drop_outcome(&store);
        assert!(kept, "{stage}: the key outside the prefix survives");
        if stage == "start" {
            assert_eq!(
                (under, entry),
                (DROPPED_KEYS, false),
                "{stage}: a drop killed before its record did nothing"
            );
        } else {
            assert_eq!(
                (under, entry),
                (1, true),
                "{stage}: the drop is finished, its entry written"
            );
        }
        drop(store);
        assert!(
            !raw_drop_state(&dir).0,
            "{stage}: the finished drop deleted its record"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
    assert_eq!(
        interrupted, 5,
        "the kills after the first five removal batches land inside the removals"
    );
}

/// A temporary keyspace left by a kill at each stage of its writing is dropped at open, and the data synced before it
/// survives.
#[test]
fn test_a_temporary_keyspace_left_by_a_kill_is_dropped_at_open() {
    for stage in TEMPORARY_STAGES {
        let dir = fresh_dir(&format!("temporary-{stage}"));
        kill_at(&dir, "temporary", stage);
        let left = usize::from(stage != "data-synced");
        assert_eq!(
            raw_temporary_keyspaces(&dir),
            left,
            "{stage}: the keyspaces the killed writer left"
        );
        let store = open(&dir);
        assert_eq!(
            store.applied_ts(),
            1,
            "{stage}: the data written beside it survives"
        );
        drop(store);
        assert_eq!(
            raw_temporary_keyspaces(&dir),
            0,
            "{stage}: the open dropped it"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A rebuild record survives a kill once it is written, and a kill before it leaves none.
#[test]
fn test_a_rebuild_record_survives_a_kill() {
    for stage in RECORD_STAGES {
        let dir = fresh_dir(&format!("record-{stage}"));
        kill_at(&dir, "record", stage);
        let store = open(&dir);
        let ids: Vec<u64> = store
            .rebuild_records()
            .expect("readable")
            .iter()
            .map(|r| r.id())
            .collect();
        let expected = if stage == "start" { vec![] } else { vec![7] };
        assert_eq!(ids, expected, "{stage}");
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
