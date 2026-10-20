/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_types::{TypeId, TypesErrorEnum, Val, convert};

#[test]
fn test_kind_and_message() {
    let err = convert(&Val::bytes(TypeId::STRING, b"x".to_vec()), TypeId::INT).unwrap_err();
    assert!(matches!(err.kind(), TypesErrorEnum::Number(_)));
    assert_eq!(
        err.message(),
        r#"strconv.ParseInt: parsing "x": invalid syntax"#
    );
    assert_eq!(err.to_string(), err.message());
    let err = convert(&Val::bytes(TypeId::INT, Vec::new()), TypeId::UID).unwrap_err();
    assert!(matches!(err.kind(), TypesErrorEnum::Conversion(_)));
    let e: &dyn std::error::Error = &err;
    assert_eq!(e.to_string(), "invalid data for int64 []");
}
