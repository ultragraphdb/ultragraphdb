/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_core_keys::{
    KeyErrorEnum, attr_in_root_namespace, data_key, parse, schema_key, split_key,
};

#[test]
fn test_kind_and_display() {
    let err = parse(&[0; 3]).expect_err("too short");
    assert_eq!(err.kind(), &KeyErrorEnum::KeyTooShort);
    assert_eq!(err.to_string(), "Key length less than 9");
    assert!(err.source().is_none());

    let err =
        split_key(&schema_key(&attr_in_root_namespace("a")), 1).expect_err("not a default prefix");
    assert_eq!(err.kind(), &KeyErrorEnum::SplitNonDefaultPrefix);

    let err = parse(&data_key(&attr_in_root_namespace("a"), 0)).expect_err("uid 0");
    assert!(matches!(err.kind(), KeyErrorEnum::ZeroUid { .. }));
    assert_eq!(
        err.to_string(),
        "Invalid UID with value 0 for key: [0 1 97 0 0 0 0 0 0 0 0 0]"
    );
}
