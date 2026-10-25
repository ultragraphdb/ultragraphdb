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
