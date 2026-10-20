/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::ValueTypeInfo;

#[test]
fn test_discriminants_match_go() {
    let all = [
        ValueTypeInfo::Unknown,
        ValueTypeInfo::Empty,
        ValueTypeInfo::Uid,
        ValueTypeInfo::Plain,
        ValueTypeInfo::Multi,
    ];
    let discriminants: Vec<i32> = all.iter().map(|v| *v as i32).collect();
    assert_eq!(discriminants, vec![0, 1, 2, 3, 4]);
    assert_eq!(ValueTypeInfo::default(), ValueTypeInfo::Unknown);
}
