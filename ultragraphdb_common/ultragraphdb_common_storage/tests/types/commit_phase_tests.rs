/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_storage::CommitPhase;

/// Every phase is listed once, in the order of its number, which indexes its histogram.
#[test]
fn test_every_phase_is_listed_in_order() {
    for (index, phase) in CommitPhase::ALL.iter().enumerate() {
        assert_eq!(*phase as usize, index);
    }
    assert_eq!(CommitPhase::ALL.len(), 6);
}
