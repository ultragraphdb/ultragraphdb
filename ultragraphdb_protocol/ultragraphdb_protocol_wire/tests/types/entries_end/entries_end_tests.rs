/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::EntriesEnd;

#[test]
fn test_each_end_has_its_byte() {
    for (end, number) in [
        (EntriesEnd::More, 0),
        (EntriesEnd::Ended, 1),
        (EntriesEnd::Limit, 2),
    ] {
        assert_eq!(end.number(), number);
        assert_eq!(EntriesEnd::from_number(number), Some(end));
    }
}

#[test]
fn test_a_byte_past_two_names_no_end() {
    for number in [3, 4, u8::MAX] {
        assert_eq!(EntriesEnd::from_number(number), None);
    }
}
