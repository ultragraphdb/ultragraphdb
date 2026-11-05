/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::Monotonic;

#[test]
fn test_each_value_keeps_its_number() {
    let all = [
        Monotonic::Snapshot,
        Monotonic::Retractable,
        Monotonic::Immutable,
    ];
    for (number, monotonic) in (0u8..).zip(all) {
        assert_eq!(monotonic.number(), number);
        assert_eq!(Monotonic::from_number(number), Some(monotonic));
    }
    assert_eq!(Monotonic::from_number(3), None);
    assert_eq!(Monotonic::default(), Monotonic::Snapshot);
}
