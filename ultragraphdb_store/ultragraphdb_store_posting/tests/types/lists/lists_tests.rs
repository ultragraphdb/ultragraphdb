/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_posting::{ListsConfig, MAX_LIST_SIZE};
use ultragraphdb_traits_store::{PersistMode, Store};

use crate::utils::helpers::{commit_edges, key, new_lists, set_uid};

#[test]
fn test_new_lists_read_at_the_stores_applied_version_and_commit_above_it() {
    let lists = new_lists();
    let ts = commit_edges(&lists, &key("start", 1), &[set_uid(1)]);
    let reopened = crate::utils::helpers::lists_over(lists.store().clone(), lists.config());
    assert_eq!(reopened.applied_ts(), ts);
    // A rollup may have written one above the newest commit: the next commit version is the even one past it.
    assert_eq!(reopened.next_version(), ts + 2);
}

/// The next version is the smallest even one above the watermark, whatever the watermark is, and saturates at the top
/// of the range, where a round at it is refused rather than wrapped.
#[test]
fn test_the_next_version_is_the_even_one_above_the_watermark() {
    let lists = new_lists();
    assert_eq!(lists.next_version(), 2, "an empty store's watermark is 0");
    for (version, next) in [
        (4, 6),
        (9, 10),
        (u64::MAX - 2, u64::MAX - 1),
        (u64::MAX - 1, u64::MAX),
    ] {
        crate::utils::helpers::commit_edges_at(
            &lists,
            &key("next", version),
            &[set_uid(1)],
            version,
        );
        assert_eq!(lists.next_version(), next, "after a commit at {version}");
    }
    let empty = new_lists().request(0).into_writes();
    crate::utils::helpers::commit_edges_at(&lists, &key("next", 1), &[set_uid(1)], u64::MAX);
    assert_eq!(lists.next_version(), u64::MAX);
    assert_eq!(
        lists.commit_round(&[(lists.next_version(), &empty)]),
        Err(ultragraphdb_store_posting::PostingError::StaleVersion(
            u64::MAX,
            u64::MAX
        ))
    );
}

/// A read moves no version: commits are ordered by the commit step, not by the reads.
#[test]
fn test_a_read_moves_no_version() {
    let lists = new_lists();
    let ts = commit_edges(&lists, &key("read", 1), &[set_uid(1)]);
    assert!(lists.try_gate(ts).unwrap());
    let _ = lists.read(&key("read", 1), ts).unwrap();
    let _open = lists.request(ts);
    assert_eq!((lists.applied_ts(), lists.next_version()), (ts, ts + 2));
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

#[test]
fn test_lists_keep_their_config_and_roll_up_at_its_split_size() {
    let config = ListsConfig::new(PersistMode::Sync, 1000);
    let lists = crate::utils::helpers::lists_over(crate::utils::helpers::new_store(), config);
    assert_eq!(lists.config(), config);
    let k = key("config-split", 1);
    commit_edges(&lists, &k, &(1..=1000).map(set_uid).collect::<Vec<_>>());
    assert!(lists.rollup_key(&k).unwrap());
    let snapshot = lists.store().snapshot(u64::MAX).unwrap();
    let (list, _) = ultragraphdb_store_posting::List::read(&k, &snapshot).unwrap();
    assert!(
        list.part_splits().len() > 1,
        "1,000 uids over 1,000 bytes split"
    );
}

#[test]
fn test_commits_are_written_in_the_configured_persist_mode_and_rollups_buffered() {
    use ultragraphdb_store_sim::OpRecord;
    let modes = |log: Vec<OpRecord>| -> Vec<PersistMode> {
        log.into_iter()
            .filter_map(|r| match r {
                OpRecord::Applied { mode, .. } => Some(mode),
                _ => None,
            })
            .collect()
    };
    let config = ListsConfig::new(PersistMode::Sync, MAX_LIST_SIZE);
    let lists = crate::utils::helpers::lists_over(crate::utils::helpers::new_store(), config);
    let k = key("persist", 1);
    lists.store().take_log();
    commit_edges(&lists, &k, &[set_uid(1)]);
    assert_eq!(modes(lists.store().take_log()), vec![PersistMode::Sync]);
    assert!(lists.rollup_key(&k).unwrap());
    assert_eq!(modes(lists.store().take_log()), vec![PersistMode::Buffer]);
    lists.sync().unwrap();
    assert!(lists.store().take_log().contains(&OpRecord::Synced));
}
