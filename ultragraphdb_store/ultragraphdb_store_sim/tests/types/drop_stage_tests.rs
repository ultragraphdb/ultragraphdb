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
