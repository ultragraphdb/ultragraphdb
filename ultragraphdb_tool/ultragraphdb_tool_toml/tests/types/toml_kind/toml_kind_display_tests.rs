/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_toml::TomlKind;

#[test]
fn test_each_kind_is_named_with_its_article() {
    assert_eq!(TomlKind::String.to_string(), "a string");
    assert_eq!(TomlKind::Integer.to_string(), "an integer");
    assert_eq!(TomlKind::Boolean.to_string(), "a boolean");
}
