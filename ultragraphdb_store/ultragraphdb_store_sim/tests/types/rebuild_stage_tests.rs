/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_sim::RebuildStage;

#[test]
fn test_the_stages_run_in_order() {
    assert!(RebuildStage::ALL.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(RebuildStage::ALL[0], RebuildStage::Start);
    assert_eq!(RebuildStage::ALL[4], RebuildStage::RecordRemoved);
}

/// Where `stage` comes in a rebuild's run; the match is exhaustive, so a stage added without a place here does not
/// compile.
fn place(stage: RebuildStage) -> usize {
    match stage {
        RebuildStage::Start => 0,
        RebuildStage::RecordWritten => 1,
        RebuildStage::RecordSynced => 2,
        RebuildStage::TemporaryWritten => 3,
        RebuildStage::RecordRemoved => 4,
    }
}

#[test]
fn test_all_holds_every_stage_once_at_its_place() {
    let places: Vec<usize> = RebuildStage::ALL.iter().map(|&s| place(s)).collect();
    assert_eq!(places, (0..5).collect::<Vec<_>>());
}
