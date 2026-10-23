/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashSet;

use ultragraphdb_error_json::{JsonError, JsonErrorEnum};

#[test]
fn test_constructors_build_their_variant() {
    assert_eq!(JsonError::Eof().0, JsonErrorEnum::Eof);
    assert_eq!(JsonError::UnexpectedEof().0, JsonErrorEnum::UnexpectedEof);
    assert_eq!(
        JsonError::Syntax("m".to_string(), 3).0,
        JsonErrorEnum::Syntax {
            msg: "m".to_string(),
            offset: 3
        }
    );
    assert_eq!(
        JsonError::UnmarshalType("v".into(), "t".into(), "s".into(), "f".into()).0,
        JsonErrorEnum::UnmarshalType {
            value: "v".to_string(),
            go_type: "t".to_string(),
            struct_name: "s".to_string(),
            field: "f".to_string(),
        }
    );
    assert_eq!(
        JsonError::Read("r".to_string()).0,
        JsonErrorEnum::Read("r".to_string())
    );
}

#[test]
fn test_the_classification_is_the_public_field() {
    let e = JsonError(JsonErrorEnum::Syntax {
        msg: "m".to_string(),
        offset: 7,
    });
    assert!(matches!(e.0, JsonErrorEnum::Syntax { offset: 7, .. }));
}

#[test]
fn test_derives_compare_clone_and_hash() {
    let e = JsonError::Eof();
    assert_eq!(e.clone(), e);
    assert_ne!(e, JsonError::UnexpectedEof());
    let set: HashSet<JsonError> = [e.clone(), e].into_iter().collect();
    assert_eq!(set.len(), 1);
    assert!(format!("{:?}", JsonError::UnexpectedEof()).contains("UnexpectedEof"));
}
