/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_toml::{TomlDocument, TomlValue};

#[test]
fn test_an_entry_holds_its_key_and_value() -> TestResult {
    let document = TomlDocument::parse(b"retries = 3\n")?;
    let entry = document
        .tables()
        .first()
        .context("the root table")?
        .entries()
        .first()
        .context("one entry")?;
    assert_eq!(entry.key(), "retries");
    assert_eq!(entry.value(), &TomlValue::Integer(3));
    Ok(())
}
