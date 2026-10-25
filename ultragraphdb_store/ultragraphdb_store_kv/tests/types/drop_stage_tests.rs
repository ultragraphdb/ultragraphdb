/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_kv::DropStage;

#[test]
fn test_a_removing_stage_carries_the_count_removed_so_far() {
    assert_eq!(DropStage::Removing(10_000), DropStage::Removing(10_000));
    assert_ne!(DropStage::Removing(10_000), DropStage::Removing(20_000));
    assert_ne!(DropStage::DataRemoved, DropStage::EntriesSynced);
    assert_eq!(format!("{:?}", DropStage::Removing(3)), "Removing(3)");
}
