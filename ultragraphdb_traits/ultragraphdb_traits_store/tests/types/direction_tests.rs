/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_store::Direction;

#[test]
fn test_a_scan_runs_forward_by_default() {
    assert_eq!(Direction::default(), Direction::Forward);
    assert_ne!(Direction::Forward, Direction::Reverse);
}
