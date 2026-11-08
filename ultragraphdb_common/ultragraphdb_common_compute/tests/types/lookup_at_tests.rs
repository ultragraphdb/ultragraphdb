/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_compute::LookupAt;

/// A lookup's snapshot gives back the epoch and the wait it was made with.
#[test]
fn test_a_lookup_reads_at_the_epoch_and_the_wait_it_was_given() {
    let at = LookupAt::new(41, 2_500);
    assert_eq!(at.epoch(), 41);
    assert_eq!(at.deadline_ms(), 2_500);
}
