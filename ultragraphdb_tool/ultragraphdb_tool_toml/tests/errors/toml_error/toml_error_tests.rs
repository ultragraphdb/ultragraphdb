/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_toml::{
    TomlError, TomlErrorEnum, TomlFeature, TomlInvalid, TomlKind, TomlLimit,
};

/// Every constructor with the variant it builds; every value distinct, so a constructor swapping two fields is caught.
fn cases() -> [(TomlError, TomlErrorEnum); 6] {
    [
        (
            TomlError::unsupported(TomlFeature::Arrays, 3),
            TomlErrorEnum::Unsupported {
                feature: TomlFeature::Arrays,
                line: 3,
            },
        ),
        (
            TomlError::invalid(TomlInvalid::DuplicateKey("sync".to_string()), 4),
            TomlErrorEnum::Invalid {
                reason: TomlInvalid::DuplicateKey("sync".to_string()),
                line: 4,
            },
        ),
        (
            TomlError::limit(TomlLimit::Keys, 4_096),
            TomlErrorEnum::Limit {
                limit: TomlLimit::Keys,
                max: 4_096,
            },
        ),
        (
            TomlError::missing("storage", "partition_count", TomlKind::Integer),
            TomlErrorEnum::Missing {
                table: "storage".to_string(),
                key: "partition_count".to_string(),
                expected: TomlKind::Integer,
            },
        ),
        (
            TomlError::wrong_kind("storage", "sync", TomlKind::Boolean, TomlKind::String),
            TomlErrorEnum::WrongKind {
                table: "storage".to_string(),
                key: "sync".to_string(),
                expected: TomlKind::Boolean,
                found: TomlKind::String,
            },
        ),
        (
            TomlError::unread_key("storage", "partiton_count"),
            TomlErrorEnum::UnreadKey {
                table: "storage".to_string(),
                key: "partiton_count".to_string(),
            },
        ),
    ]
}

#[test]
fn test_each_constructor_builds_its_variant_with_its_values() {
    for (built, expected) in cases() {
        assert_eq!(built.0, expected);
    }
}

#[test]
fn test_errors_compare_by_variant_and_values() {
    assert_eq!(
        TomlError::unsupported(TomlFeature::Floats, 2),
        TomlError::unsupported(TomlFeature::Floats, 2)
    );
    assert_ne!(
        TomlError::unsupported(TomlFeature::Floats, 2),
        TomlError::unsupported(TomlFeature::Floats, 3)
    );
    assert_ne!(
        TomlError::unsupported(TomlFeature::Floats, 2),
        TomlError::unsupported(TomlFeature::Arrays, 2)
    );
    assert_ne!(
        TomlError::missing("", "name", TomlKind::String),
        TomlError::unread_key("", "name")
    );
    let error = TomlError::limit(TomlLimit::Tables, 256);
    assert_eq!(error.clone(), error);
}
