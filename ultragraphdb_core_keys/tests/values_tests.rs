/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::{ValueTypeInfo, value_type};

#[test]
fn test_value_type() {
    // (has_value, has_lang, has_special_id) -> type, every combination.
    let cases = [
        ((true, true, true), ValueTypeInfo::Multi),
        ((true, true, false), ValueTypeInfo::Multi),
        ((true, false, true), ValueTypeInfo::Plain),
        ((true, false, false), ValueTypeInfo::Plain),
        ((false, true, true), ValueTypeInfo::Empty),
        ((false, false, true), ValueTypeInfo::Empty),
        ((false, true, false), ValueTypeInfo::Uid),
        ((false, false, false), ValueTypeInfo::Uid),
    ];
    for ((has_value, has_lang, has_special_id), expected) in cases {
        assert_eq!(
            value_type(has_value, has_lang, has_special_id),
            expected,
            "{has_value} {has_lang} {has_special_id}"
        );
    }
}
