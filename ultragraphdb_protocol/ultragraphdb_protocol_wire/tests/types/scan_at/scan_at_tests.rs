/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::ScanAt;
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_each_choice_has_its_byte_and_no_other_byte_names_one() -> TestResult {
    let all = [
        (ScanAt::ReadVersion, 1, true),
        (ScanAt::Latest, 2, false),
        (ScanAt::LatestClosed, 3, false),
        (ScanAt::Named, 4, true),
    ];
    for (at, byte, names) in all {
        assert_eq!(at.number(), byte);
        assert_eq!(ScanAt::from_number(byte), Some(at));
        assert_eq!(at.names_version(), names);
    }
    for byte in [0u8, 5, 255] {
        assert_eq!(ScanAt::from_number(byte), None);
    }
    Ok(())
}
