/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A read's page holds each object encoding once, however many statement IDs its record lists: what bounds a page by the
//! frame it was read from (design D3; `notes/group-5-lift-compute.md`, *Found wanting*).

use ultragraphdb_common_compute::page_held_bytes;
use ultragraphdb_core_keys::Graph;
use ultragraphdb_protocol_wire::ObjectRecordsBuilder;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::apart::apart_reads::{forward_in, id};

/// A vector object of 65,535 four-byte elements, the largest `VECTOR_MAX` admits, under 64 statement IDs: the page holds its
/// 262,140 bytes once, where a copy per statement ID held 64 times as many.
#[test]
fn test_a_record_s_encoding_is_held_once_for_all_its_statement_ids() -> TestResult {
    // 65,535 elements of four bytes each: the encoding's length, by the lift note's arithmetic.
    let encoding = vec![0x5A; 65_535 * 4];
    let statements: Vec<[u8; 16]> = (1..=64)
        .map(|sequence| id(1_000 + sequence).map(|held| held.to_be_bytes()))
        .collect::<TestResult<_>>()?;
    let mut records = ObjectRecordsBuilder::new();
    records.push(&encoding, &statements);
    let key = forward_in(Graph::Default, id(1)?, id(2)?);
    let held = page_held_bytes(&key, records.value()).context("the page reads the record")?;
    assert_eq!(held, 262_140);
    Ok(())
}

/// Two records, one without statement IDs and one with three, hold their two encodings once each.
#[test]
fn test_each_record_s_encoding_is_held_once() -> TestResult {
    let (first, second) = (vec![1u8; 100], vec![2u8; 300]);
    let statements: Vec<[u8; 16]> = (1..=3)
        .map(|sequence| id(2_000 + sequence).map(|held| held.to_be_bytes()))
        .collect::<TestResult<_>>()?;
    let mut records = ObjectRecordsBuilder::new();
    records.push(&first, &[]);
    records.push(&second, &statements);
    let key = forward_in(Graph::Default, id(1)?, id(2)?);
    let held = page_held_bytes(&key, records.value()).context("the page reads the records")?;
    // 100 bytes and 300 bytes, each once.
    assert_eq!(held, 400);
    Ok(())
}
