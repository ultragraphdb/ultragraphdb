/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_toml::TomlLimit;

#[test]
fn test_each_limit_is_named_by_its_unit() {
    assert_eq!(TomlLimit::DocumentBytes.to_string(), "bytes");
    assert_eq!(TomlLimit::LineBytes.to_string(), "bytes per line");
    assert_eq!(TomlLimit::Tables.to_string(), "tables");
    assert_eq!(TomlLimit::Keys.to_string(), "keys");
}
