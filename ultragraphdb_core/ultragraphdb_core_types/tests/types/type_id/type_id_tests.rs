/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Dgraph's `TypeID` (types/scalar_types.go): the stored identifiers, `Name`, `TypeForName`, `IsScalar` and `IsNumber`.

use ultragraphdb_core_types::{TYPE_NAMES, TypeId};

/// Every identifier with its `pb.Posting_ValType` value and Go's `Name`.
const IDS: [(TypeId, i32, &str); 14] = [
    (TypeId::DEFAULT, 0, "default"),
    (TypeId::BINARY, 1, "binary"),
    (TypeId::INT, 2, "int"),
    (TypeId::FLOAT, 3, "float"),
    (TypeId::BOOL, 4, "bool"),
    (TypeId::DATETIME, 5, "datetime"),
    (TypeId::GEO, 6, "geo"),
    (TypeId::UID, 7, "uid"),
    (TypeId::PASSWORD, 8, "password"),
    (TypeId::STRING, 9, "string"),
    (TypeId::OBJECT, 10, ""),
    (TypeId::BIGFLOAT, 11, "bigfloat"),
    (TypeId::VFLOAT, 12, "float32vector"),
    (TypeId::UNDEFINED, 100, ""),
];

#[test]
fn test_stored_values_and_names() {
    for (tid, value, name) in IDS {
        assert_eq!(tid.enum_value(), value, "{name}");
        assert_eq!(TypeId::new(value), tid, "{name}");
        assert_eq!(tid.name(), name, "{value}");
    }
    // Any i32 is an identifier; those without a name have the empty one.
    for value in [-1, 13, 99, 101, i32::MIN, i32::MAX] {
        let tid = TypeId::new(value);
        assert_eq!(tid.enum_value(), value);
        assert_eq!(tid.name(), "", "{value}");
    }
}

#[test]
fn test_type_for_name() {
    // Go's typeNameMap: twelve names, OBJECT and UNDEFINED have none.
    let want = [
        ("default", 0),
        ("binary", 1),
        ("int", 2),
        ("float", 3),
        ("bool", 4),
        ("datetime", 5),
        ("geo", 6),
        ("uid", 7),
        ("string", 9),
        ("password", 8),
        ("bigfloat", 11),
        ("float32vector", 12),
    ];
    let names: Vec<(&str, i32)> = TYPE_NAMES
        .iter()
        .map(|(n, t)| (*n, t.enum_value()))
        .collect();
    assert_eq!(names, want);
    for (name, value) in want {
        assert_eq!(TypeId::for_name(name), Some(TypeId::new(value)), "{name}");
    }
    // The lookup is exact: no case folding, no trimming, no aliases.
    for name in [
        "",
        "Int",
        "INT",
        " int",
        "int ",
        "object",
        "undefined",
        "vfloat",
        "float32",
        "uid\0",
    ] {
        assert_eq!(TypeId::for_name(name), None, "{name:?}");
    }
}

#[test]
fn test_is_scalar_and_is_number() {
    for (tid, value, name) in IDS {
        assert_eq!(tid.is_scalar(), value != 7, "{name}");
        assert_eq!(tid.is_number(), value == 2 || value == 3, "{name}");
    }
    assert!(TypeId::new(-1).is_scalar());
    assert!(!TypeId::new(-1).is_number());
    assert!(TypeId::INT < TypeId::FLOAT && TypeId::VFLOAT < TypeId::UNDEFINED);
}
