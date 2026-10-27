/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The stages a drop reports: a `Removing` stage after each full batch of 10,000 removals, with the count removed so
//! far, and none for the last batch when it is short.

use ultragraphdb_store_kv::DropStage;
use ultragraphdb_traits_store::Entry;

use ultragraphdb_tool_test::{TestDir, put, write};

fn stages_of_a_drop_over(keys: u32) -> Vec<DropStage> {
    let dir = TestDir::new(&format!("drop-stage-{keys}"));
    let s = dir.open();
    let entries: Vec<Entry> = (0..keys)
        .map(|i| put(&[b"\x00z".as_slice(), &i.to_be_bytes()].concat(), 2, b""))
        .collect();
    if !entries.is_empty() {
        write(&s, entries);
    }
    let mut stages = Vec::new();
    s.drop_prefixes_observed(9, &[b"\x00z".to_vec()], Vec::new(), |stage| {
        stages.push(stage)
    })
    .unwrap();
    assert!(
        s.dump().unwrap().is_empty(),
        "{keys} keys: every one removed"
    );
    stages
}

#[test]
fn test_a_removing_stage_follows_each_full_batch_of_removals() {
    use DropStage::{DataRemoved, EntriesSynced, RecordSynced, Removing};
    for (keys, expected) in [
        (0, vec![RecordSynced, DataRemoved, EntriesSynced]),
        (9_999, vec![RecordSynced, DataRemoved, EntriesSynced]),
        (
            10_000,
            vec![RecordSynced, Removing(10_000), DataRemoved, EntriesSynced],
        ),
        (
            10_001,
            vec![RecordSynced, Removing(10_000), DataRemoved, EntriesSynced],
        ),
        (
            20_000,
            vec![
                RecordSynced,
                Removing(10_000),
                Removing(20_000),
                DataRemoved,
                EntriesSynced,
            ],
        ),
    ] {
        assert_eq!(stages_of_a_drop_over(keys), expected, "{keys} keys");
    }
}
