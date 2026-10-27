/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Kills the writer at each stage of a rebuild record's write and reopens the store: a synced rebuild record
//! survives the kill.

use ultragraphdb_tool_test::{RECORD_STAGES, fresh_dir, kill_at, open};
use ultragraphdb_traits_store::Store;

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
