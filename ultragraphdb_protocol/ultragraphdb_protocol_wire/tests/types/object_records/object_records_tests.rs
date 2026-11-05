/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, EntriesView, ObjectRecords};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::entries::entries_tests::{Object, list_a, list_b_first, list_b_rest, value_of};
use crate::utils::golden_bodies::frame_of;
use crate::utils::golden_roles::{golden_role, role_bytes};

/// The objects `value` holds.
fn objects_of(value: &[u8]) -> TestResult<Vec<Object>> {
    Ok(ObjectRecords::read(value)?
        .iter()
        .map(|record| (record.encoding().to_vec(), record.statement_ids().collect()))
        .collect())
}

/// Each entry's value in the golden frame `name`.
fn golden_values(name: &str) -> TestResult<Vec<Vec<u8>>> {
    let frame = role_bytes(&golden_role(name)?)?;
    let view = EntriesView::from_frame(&frame_of(&frame)?)?;
    Ok(view
        .entries()
        .iter()
        .filter_map(|entry| entry.value().map(<[u8]>::to_vec))
        .collect())
}

/// The refusal of a record running past its value: record `entry` at offset `offset` of the value.
fn out_of_bounds(entry: usize, offset: usize) -> ProtocolErrorEnum {
    ProtocolErrorEnum::EntryOutOfBounds {
        command: 525,
        section: 0,
        entry,
        offset,
    }
}

fn refused(value: &[u8]) -> TestResult<ProtocolErrorEnum> {
    Ok(ObjectRecords::read(value).err().context("a refusal")?.0)
}

#[test]
fn test_the_golden_values_read_back_to_their_objects() -> TestResult {
    assert_eq!(
        golden_values("entries_continues")?
            .iter()
            .map(|value| objects_of(value))
            .collect::<TestResult<Vec<_>>>()?,
        [list_a(), list_b_first()]
    );
    assert_eq!(
        golden_values("entries_continued")?
            .iter()
            .map(|value| objects_of(value))
            .collect::<TestResult<Vec<_>>>()?,
        [list_b_rest()]
    );
    Ok(())
}

#[test]
fn test_the_records_of_every_frame_in_order_are_the_list() -> TestResult {
    let frames = [
        golden_values("entries_continues")?
            .last()
            .context("the continued value")?
            .clone(),
        golden_values("entries_continued")?
            .first()
            .context("the next value")?
            .clone(),
    ];
    let list = [list_b_first(), list_b_rest()].concat();
    assert_eq!(objects_of(&frames.concat())?, list);
    assert_eq!(frames.concat(), value_of(&list));
    Ok(())
}

#[test]
fn test_a_value_of_no_records_is_empty() -> TestResult {
    let records = ObjectRecords::read(&[])?;
    assert!(records.is_empty());
    assert_eq!(records.len(), 0);
    assert_eq!(records.iter().count(), 0);
    Ok(())
}

#[test]
fn test_the_count_and_the_walk_agree() -> TestResult {
    let objects = list_b_rest();
    let value = value_of(&objects);
    let records = ObjectRecords::read(&value)?;
    assert!(!records.is_empty());
    assert_eq!(records.len(), objects.len());
    assert_eq!(records.iter().count(), objects.len());
    Ok(())
}

#[test]
fn test_every_truncation_of_a_value_is_refused_naming_its_last_record() -> TestResult {
    let objects = list_b_rest();
    let first = value_of(objects.get(..1).context("the first object")?);
    let value = value_of(&objects);
    for len in 1..value.len() {
        let cut = value.get(..len).context("a prefix")?;
        if len == first.len() {
            assert_eq!(objects_of(cut)?, objects.get(..1).context("it")?);
        } else if len < first.len() {
            assert_eq!(refused(cut)?, out_of_bounds(0, 0));
        } else {
            assert_eq!(refused(cut)?, out_of_bounds(1, first.len()));
        }
    }
    Ok(())
}

#[test]
fn test_lengths_past_the_value_are_refused_without_a_walk() -> TestResult {
    let mut encoding_past = value_of(&list_a());
    encoding_past[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(refused(&encoding_past)?, out_of_bounds(0, 0));
    let mut statements_past = value_of(&list_a());
    statements_past[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(refused(&statements_past)?, out_of_bounds(0, 0));
    let mut one_statement_short = value_of(&list_a());
    one_statement_short[4..8].copy_from_slice(&1u32.to_le_bytes());
    assert_eq!(refused(&one_statement_short)?, out_of_bounds(0, 0));
    Ok(())
}
