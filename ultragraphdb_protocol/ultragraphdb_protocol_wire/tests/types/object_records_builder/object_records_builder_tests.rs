/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::ObjectRecordsBuilder;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::entries::entries_tests::list_b_first;

#[test]
fn test_a_builder_starts_with_no_records() {
    assert!(ObjectRecordsBuilder::new().value().is_empty());
}

#[test]
fn test_a_record_is_laid_out_from_the_protocols_table() -> TestResult {
    let objects = list_b_first();
    let (encoding, statements) = objects.first().context("an object")?;
    let mut builder = ObjectRecordsBuilder::new();
    builder.push(encoding, statements);
    let mut expected = (encoding.len() as u32).to_le_bytes().to_vec();
    expected.extend_from_slice(&(statements.len() as u32).to_le_bytes());
    expected.extend_from_slice(encoding);
    statements
        .iter()
        .for_each(|id| expected.extend_from_slice(id));
    assert_eq!(builder.value(), expected);
    Ok(())
}

#[test]
fn test_records_follow_each_other_with_no_gap() -> TestResult {
    let objects = list_b_first();
    let (encoding, statements) = objects.first().context("an object")?;
    let mut one = ObjectRecordsBuilder::new();
    one.push(encoding, statements);
    let mut two = one.clone();
    two.push(encoding, statements);
    assert_eq!(two.value(), [one.value(), one.value()].concat());
    Ok(())
}
