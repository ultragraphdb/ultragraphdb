/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_sim::DropStage;

#[test]
fn test_the_stages_run_in_order() {
    assert!(DropStage::ALL.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(DropStage::ALL[0], DropStage::Start);
    assert_eq!(DropStage::ALL[4], DropStage::EntriesSynced);
}

/// Where `stage` comes in a drop's run; the match is exhaustive, so a stage added without a place here does not
/// compile.
fn place(stage: DropStage) -> usize {
    match stage {
        DropStage::Start => 0,
        DropStage::RecordWritten => 1,
        DropStage::RecordSynced => 2,
        DropStage::DataRemoved => 3,
        DropStage::EntriesSynced => 4,
    }
}

#[test]
fn test_all_holds_every_stage_once_at_its_place() {
    let places: Vec<usize> = DropStage::ALL.iter().map(|&s| place(s)).collect();
    assert_eq!(places, (0..5).collect::<Vec<_>>());
}
