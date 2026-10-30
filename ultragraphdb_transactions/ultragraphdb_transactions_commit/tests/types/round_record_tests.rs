/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_transactions_commit::RoundRecord;

#[test]
fn test_a_record_keeps_its_fields() {
    let record = RoundRecord::new(10, 3, 2);
    assert_eq!(record.first_version(), 10);
    assert_eq!(record.transactions(), 3);
    assert_eq!(record.committed(), 2);
}

#[test]
fn test_the_last_version_is_two_per_commit_after_the_first() {
    // Committed k has first + 2k: two commits from 10 end at 12, five at 18.
    assert_eq!(RoundRecord::new(10, 3, 2).last_version(), Some(12));
    assert_eq!(RoundRecord::new(10, 5, 5).last_version(), Some(18));
    assert_eq!(RoundRecord::new(10, 1, 1).last_version(), Some(10));
    assert_eq!(RoundRecord::new(10, 4, 0).last_version(), None);
    // Past u64::MAX there is no last version.
    assert_eq!(RoundRecord::new(u64::MAX - 1, 2, 2).last_version(), None);
    // At it, the largest even version is a last version still.
    assert_eq!(
        RoundRecord::new(u64::MAX - 1, 1, 1).last_version(),
        Some(u64::MAX - 1)
    );
    assert_eq!(
        RoundRecord::new(u64::MAX - 3, 2, 2).last_version(),
        Some(u64::MAX - 1)
    );
}
