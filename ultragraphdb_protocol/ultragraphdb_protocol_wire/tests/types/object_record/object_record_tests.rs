/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::ObjectRecords;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::entries::entries_tests::{list_a, list_b_first, value_of};

#[test]
fn test_a_record_gives_its_encoding_and_statement_ids() -> TestResult {
    let objects = list_b_first();
    let value = value_of(&objects);
    let record = ObjectRecords::read(&value)?
        .iter()
        .next()
        .context("a record")?;
    let (encoding, statements) = objects.first().context("an object")?;
    assert_eq!(record.encoding(), encoding.as_slice());
    assert_eq!(record.statement_count(), statements.len());
    assert_eq!(&record.statement_ids().collect::<Vec<_>>(), statements);
    Ok(())
}

#[test]
fn test_a_record_held_under_no_statement_has_no_ids() -> TestResult {
    let value = value_of(&list_a());
    let record = ObjectRecords::read(&value)?
        .iter()
        .next()
        .context("a record")?;
    assert_eq!(record.statement_count(), 0);
    assert_eq!(record.statement_ids().count(), 0);
    Ok(())
}
