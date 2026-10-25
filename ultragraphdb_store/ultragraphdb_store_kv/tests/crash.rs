/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Kills a process writing into a fjall store, during a batch and during a drop, and reopens the store: a batch is
//! present whole or absent whole, the batches present are a prefix of batch order with the applied timestamp of the
//! last, and a drop whose record was synced is finished at open.

mod crash_harness;

use std::time::Duration;

use ultragraphdb_traits_store::Store;

use crate::crash_harness::{
    DROPPED_KEYS, KEYS_PER_BATCH, batch_counts, drop_outcome, fresh_dir, kill_after, open,
    raw_drop_state, raw_temporary_keyspaces,
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

#[test]
fn test_a_kill_during_a_drop_is_finished_at_open() {
    let mut interrupted = 0;
    for (round, delay) in [0u64, 30, 90, 200].into_iter().enumerate() {
        let dir = fresh_dir(&format!("drop-{round}"));
        let lines = kill_after(&dir, "drop", "dropping", Duration::from_millis(delay));
        let finished = lines.iter().any(|l| l == "dropped");
        let (record, left) = raw_drop_state(&dir);
        println!(
            "drop round {round}: killed {delay} ms after it began, record {record}, {left} keys left"
        );
        if record && left > 0 && left < DROPPED_KEYS {
            interrupted += 1;
        }
        let store = open(&dir);
        let (under, entry, kept) = drop_outcome(&store);
        assert!(kept, "round {round}: the key outside the prefix survives");
        if record || finished {
            assert_eq!(
                (under, entry),
                (1, true),
                "round {round}: the drop is finished, its entry written"
            );
        } else {
            assert!(
                under == DROPPED_KEYS && !entry,
                "round {round}: a drop killed before its record did nothing"
            );
        }
        drop(store);
        assert!(
            !raw_drop_state(&dir).0,
            "round {round}: the finished drop deleted its record"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
    println!(
        "{interrupted} of 4 kills landed after the drop record was synced and before its removals ended"
    );
    assert!(interrupted > 0, "no kill landed inside the drop's removals");
}

#[test]
fn test_a_temporary_keyspace_left_by_a_kill_is_dropped_at_open() {
    let dir = fresh_dir("temporary");
    let lines = kill_after(
        &dir,
        "temporary",
        "temporary written",
        Duration::from_millis(0),
    );
    assert!(lines.iter().any(|l| l == "temporary written"));
    assert_eq!(
        raw_temporary_keyspaces(&dir),
        1,
        "the killed writer left its keyspace"
    );
    let store = open(&dir);
    assert_eq!(store.applied_ts(), 1, "the data written beside it survives");
    drop(store);
    assert_eq!(raw_temporary_keyspaces(&dir), 0, "the open dropped it");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_a_rebuild_record_survives_a_kill() {
    let dir = fresh_dir("record");
    kill_after(&dir, "record", "recorded", Duration::from_millis(0));
    let store = open(&dir);
    let records = store.rebuild_records().expect("readable");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].id(), 7);
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}
