/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_posting::{ListsConfig, MAX_LIST_SIZE};
use ultragraphdb_traits_store::PersistMode;

use crate::utils::helpers::{commit_edges, key, new_lists, set_uid};

#[test]
fn test_new_lists_start_above_the_stores_applied_timestamp() {
    let lists = new_lists();
    let ts = commit_edges(&lists, &key("start", 1), &[set_uid(1)]);
    let reopened = crate::utils::helpers::lists_over(lists.store().clone(), lists.config());
    // A rollup may have written one above the newest commit, so the counter starts two above.
    assert_eq!(reopened.peek_ts(), ts + 2);
    assert_eq!(reopened.applied_ts(), ts);
}

#[test]
fn test_the_gate_moves_the_counter_past_a_read() {
    let lists = new_lists();
    lists.gate(40).unwrap();
    assert_eq!(lists.peek_ts(), 41);
    assert!(lists.try_gate(1000).unwrap());
    let fresh = lists.fresh_ts();
    assert_eq!((fresh, lists.peek_ts()), (41, 42));
}

#[test]
fn test_the_default_config_is_process_durability_at_dgraphs_split_size() {
    let config = ListsConfig::default();
    assert_eq!(
        (config.persist(), config.max_list_size()),
        (PersistMode::Buffer, MAX_LIST_SIZE)
    );
    let lists = new_lists();
    assert_eq!(lists.config(), config);
    assert_eq!(lists.epoch(), 0);
    lists.sync().unwrap();
}
