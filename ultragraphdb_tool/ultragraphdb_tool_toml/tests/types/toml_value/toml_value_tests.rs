/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_toml::TomlKind;
use ultragraphdb_tool_toml::TomlValue;

#[test]
fn test_each_value_names_its_kind() {
    assert_eq!(TomlValue::String("4".to_string()).kind(), TomlKind::String);
    assert_eq!(TomlValue::Integer(4).kind(), TomlKind::Integer);
    assert_eq!(TomlValue::Boolean(false).kind(), TomlKind::Boolean);
}
