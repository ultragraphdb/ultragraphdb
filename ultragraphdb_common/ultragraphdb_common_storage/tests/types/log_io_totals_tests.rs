/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_storage::LogIoTotals;

/// What was spent between two totals, each field apart, and nothing where the earlier lies past the later.
#[test]
fn test_since_takes_each_field_apart() {
    let earlier = LogIoTotals::new((10, 1, 4_096), (300, 2));
    let later = LogIoTotals::new((25, 3, 12_288), (700, 3));
    let spent = later.since(&earlier);
    assert_eq!(spent, LogIoTotals::new((15, 2, 8_192), (400, 1)));
    assert_eq!(earlier.since(&later), LogIoTotals::default());
    assert_eq!(spent.write_us(), 15);
    assert_eq!(spent.writes(), 2);
    assert_eq!(spent.written_bytes(), 8_192);
    assert_eq!(spent.sync_us(), 400);
    assert_eq!(spent.syncs(), 1);
}
