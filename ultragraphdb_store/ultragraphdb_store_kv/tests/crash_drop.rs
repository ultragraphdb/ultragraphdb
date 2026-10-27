/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Kills the writer at each stage of a prefix drop and reopens the store: a drop whose record was synced is finished
//! at open.

use ultragraphdb_tool_test::{
    DROP_STAGES, DROPPED_KEYS, drop_outcome, fresh_dir, kill_at, open, raw_drop_state,
};

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
