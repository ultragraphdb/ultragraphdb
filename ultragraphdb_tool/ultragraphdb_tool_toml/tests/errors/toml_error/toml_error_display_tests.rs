/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_toml::{TomlError, TomlFeature, TomlInvalid, TomlKind, TomlLimit};

// No other parser's text applies: the subset is the repository's own. Each text names the line, the feature or rule,
// and a setting as `table.key`, or the key alone in the root table.

#[test]
fn test_parser_texts_name_the_line() {
    assert_eq!(
        TomlError::unsupported(TomlFeature::Arrays, 3).to_string(),
        "line 3: arrays are outside the TOML subset settings files use"
    );
    assert_eq!(
        TomlError::invalid(TomlInvalid::DuplicateKey("sync".to_string()), 4).to_string(),
        "line 4: invalid TOML: the key `sync` defined twice"
    );
}

#[test]
fn test_limit_texts_name_the_bound() {
    assert_eq!(
        TomlError::limit(TomlLimit::DocumentBytes, 65_536).to_string(),
        "the settings file passes its limit of 65536 bytes"
    );
    assert_eq!(
        TomlError::limit(TomlLimit::LineBytes, 4_096).to_string(),
        "the settings file passes its limit of 4096 bytes per line"
    );
    assert_eq!(
        TomlError::limit(TomlLimit::Tables, 256).to_string(),
        "the settings file passes its limit of 256 tables"
    );
    assert_eq!(
        TomlError::limit(TomlLimit::Keys, 4_096).to_string(),
        "the settings file passes its limit of 4096 keys"
    );
}

#[test]
fn test_read_texts_name_the_setting() {
    assert_eq!(
        TomlError::missing("storage", "partition_count", TomlKind::Integer).to_string(),
        "the setting storage.partition_count is missing; an integer is expected"
    );
    assert_eq!(
        TomlError::wrong_kind(
            "storage",
            "partition_count",
            TomlKind::Integer,
            TomlKind::String
        )
        .to_string(),
        "the setting storage.partition_count must be an integer, not a string"
    );
    assert_eq!(
        TomlError::unread_key("storage", "partiton_count").to_string(),
        "the setting storage.partiton_count is unknown"
    );
}

#[test]
fn test_a_root_setting_is_named_by_its_key_alone() {
    assert_eq!(
        TomlError::missing("", "name", TomlKind::String).to_string(),
        "the setting name is missing; a string is expected"
    );
    assert_eq!(
        TomlError::wrong_kind("", "sync", TomlKind::Boolean, TomlKind::Integer).to_string(),
        "the setting sync must be a boolean, not an integer"
    );
    assert_eq!(
        TomlError::unread_key("", "nmae").to_string(),
        "the setting nmae is unknown"
    );
}
