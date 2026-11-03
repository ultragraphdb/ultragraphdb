/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::ReportKind;

#[test]
fn test_each_kind_has_its_byte() {
    for (kind, number) in [
        (ReportKind::Passed, 1),
        (ReportKind::Refused, 2),
        (ReportKind::BuildFailed, 3),
    ] {
        assert_eq!(kind.number(), number);
        assert_eq!(ReportKind::from_number(number), Some(kind));
    }
}

#[test]
fn test_zero_and_bytes_past_three_name_no_kind() {
    for number in [0, 4, u8::MAX] {
        assert_eq!(ReportKind::from_number(number), None);
    }
}
