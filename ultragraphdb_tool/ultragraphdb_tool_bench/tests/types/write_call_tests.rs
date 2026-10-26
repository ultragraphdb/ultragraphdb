/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bench::WriteCall;

#[test]
fn test_a_write_call_keeps_what_it_was_made_of() {
    let call = WriteCall::new(1_500, 12, true, 3, 1);
    assert_eq!(
        (
            call.nanos(),
            call.entries(),
            call.commit(),
            call.l0_tables(),
            call.sealed_memtables()
        ),
        (1_500, 12, true, 3, 1)
    );
    assert!(!WriteCall::new(1, 1, false, 0, 0).commit());
}

#[test]
fn test_pressure_starts_at_twenty_level_0_tables_or_four_sealed_memtables() {
    // fjall's thresholds, as the type documents them: 20 level-0 tables, 4 sealed memtables.
    let pressured = |l0, sealed| WriteCall::new(0, 0, true, l0, sealed).under_pressure();
    assert!(!pressured(0, 0));
    assert!(!pressured(19, 3));
    assert!(pressured(20, 0));
    assert!(pressured(0, 4));
    assert!(pressured(20, 4));
    assert!(pressured(usize::MAX, 0));
}
