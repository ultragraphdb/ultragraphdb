/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_toml::{TomlDocument, TomlValue};

#[test]
fn test_a_table_holds_its_name_and_entries_in_order() -> TestResult {
    let document = TomlDocument::parse(b"[storage]\nsync = true\npath = \"/data\"\n")?;
    let table = document.tables().get(1).context("the storage table")?;
    assert_eq!(table.name(), "storage");
    let keys: Vec<&str> = table.entries().iter().map(|e| e.key()).collect();
    assert_eq!(keys, ["sync", "path"]);
    let values: Vec<&TomlValue> = table.entries().iter().map(|e| e.value()).collect();
    assert_eq!(
        values,
        [
            &TomlValue::Boolean(true),
            &TomlValue::String("/data".to_string())
        ]
    );
    Ok(())
}

#[test]
fn test_the_root_table_has_the_empty_name() -> TestResult {
    let document = TomlDocument::parse(b"[storage]\n")?;
    assert_eq!(
        document.tables().first().context("the root table")?.name(),
        ""
    );
    Ok(())
}
