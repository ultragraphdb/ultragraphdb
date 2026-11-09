/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_storage::{LogIo, LogIoTotals};

/// Writes and syncs add to the totals, which every clone shares.
#[test]
fn test_writes_and_syncs_add_to_shared_totals() {
    let io = LogIo::default();
    assert_eq!(io.totals(), LogIoTotals::default());
    let clone = io.clone();
    io.wrote(4_096, 30);
    clone.wrote(8_192, 50);
    clone.synced(400);
    let totals = io.totals();
    assert_eq!(totals.writes(), 2);
    assert_eq!(totals.written_bytes(), 12_288);
    assert_eq!(totals.write_us(), 80);
    assert_eq!(totals.syncs(), 1);
    assert_eq!(totals.sync_us(), 400);
    assert_eq!(clone.totals(), totals);
}

/// A total stays at its largest value past it.
#[test]
fn test_a_total_saturates() {
    let io = LogIo::default();
    io.synced(u64::MAX);
    io.synced(1);
    assert_eq!(io.totals().sync_us(), u64::MAX);
    assert_eq!(io.totals().syncs(), 2);
}
