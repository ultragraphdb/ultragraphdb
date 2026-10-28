/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_transport::NetworkStats;

#[test]
fn test_fresh_stats_count_nothing() {
    let stats = NetworkStats::default();
    assert_eq!(
        (
            stats.sent(),
            stats.dropped(),
            stats.duplicated(),
            stats.delayed(),
            stats.split()
        ),
        (0, 0, 0, 0, 0)
    );
}
