/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use fjall::{Database, KeyspaceCreateOptions};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_test::{TestDir, tenant_key, write_raw_at};

#[test]
fn test_a_raw_write_lands_at_its_key_of_its_keyspace() -> TestResult {
    let dir = TestDir::new("raw-at");
    write_raw_at(dir.path(), "a_keyspace", b"a key", b"a value")?;
    let db = Database::builder(dir.path())
        .open()
        .context("fjall opens")?;
    let ks = db
        .keyspace("a_keyspace", KeyspaceCreateOptions::default)
        .context("the keyspace opens")?;
    let held = ks.get(b"a key").context("a read")?.context("the value")?;
    assert_eq!(held.as_ref(), b"a value");
    Ok(())
}

#[test]
fn test_a_tenant_key_starts_with_the_byte_and_the_tenant() {
    // The data region's byte, then the tenant as eight big-endian bytes, then the rest.
    assert_eq!(tenant_key(0x0102, b"x"), [0, 0, 0, 0, 0, 0, 0, 1, 2, b'x']);
}
