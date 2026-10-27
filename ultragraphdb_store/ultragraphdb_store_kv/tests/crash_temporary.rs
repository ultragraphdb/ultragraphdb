/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Kills the writer at each stage of a temporary keyspace's life and reopens the store: a temporary keyspace a kill
//! left behind is dropped at open.

use ultragraphdb_tool_test::{TEMPORARY_STAGES, fresh_dir, kill_at, open, raw_temporary_keyspaces};
use ultragraphdb_traits_store::Store;

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
