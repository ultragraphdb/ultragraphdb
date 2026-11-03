/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    ABSENT, OperationResult, RecordEntry, RecordKind, RecordOperation, WriteSetRecordView,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::write_set_record::write_set_record_tests::{
    SESSION, record0, record1, record2, record3, registration, sid,
};
use crate::utils::golden_bodies::{body_data, golden, join_sections, put, section, split_sections};

/// The Round command, which a record's refusals name.
const ROUND: u16 = 17;

/// The golden Round's records, cut from its body by its record table.
pub fn golden_records() -> TestResult<Vec<Vec<u8>>> {
    let bytes = golden("round")?.bytes()?;
    let data = body_data(&bytes).to_vec();
    let (table, table_len) = section(&data, 0, 1);
    let (records, _) = section(&data, 0, 2);
    (0..table_len / 8)
        .map(|i| -> TestResult<Vec<u8>> {
            let at = table + 8 * i;
            let offset = u32::from_le_bytes(data[at..at + 4].try_into()?) as usize;
            let len = u32::from_le_bytes(data[at + 4..at + 8].try_into()?) as usize;
            Ok(data[records + offset..records + offset + len].to_vec())
        })
        .collect()
}

/// Golden record `index` with its fixed part and sections changed by `change`, laid out again.
fn changed(
    index: usize,
    change: impl FnOnce(&mut Vec<u8>, &mut Vec<Vec<u8>>),
) -> TestResult<Vec<u8>> {
    let record = golden_records()?
        .get(index)
        .cloned()
        .context("a golden record")?;
    let mut fixed = record[..64].to_vec();
    let mut secs = split_sections(&record, 64, 12);
    change(&mut fixed, &mut secs);
    Ok(join_sections(&fixed, &secs))
}

/// The refusal of `bytes`.
fn refusal(bytes: &[u8]) -> TestResult<ProtocolErrorEnum> {
    Ok(WriteSetRecordView::new(bytes).err().context("a refusal")?.0)
}

/// A key-value entry laid out from the layout table: lengths, key, value, zeros to 8.
fn entry(key: &[u8], value: Option<&[u8]>) -> Vec<u8> {
    let mut out = (key.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(&value.map_or(ABSENT, |v| v.len() as u32).to_le_bytes());
    out.extend_from_slice(key);
    out.extend_from_slice(value.unwrap_or_default());
    out.resize(out.len().div_ceil(8) * 8, 0);
    out
}

#[test]
fn test_the_golden_transaction_reads_its_values() -> TestResult {
    let records = golden_records()?;
    let view = WriteSetRecordView::new(&records[0])?;
    assert_eq!(view.bytes(), &records[0][..]);
    assert_eq!(view.kind(), RecordKind::Transaction);
    assert_eq!(
        (
            view.namespace(),
            view.catalog(),
            view.read_version(),
            view.request(),
            view.principal()
        ),
        (3, 11, (7 << 22) | 50, 7, 9)
    );
    assert_eq!(view.session(), SESSION);
    let id5 = sid(2, 5, 0x2A)?.to_be_bytes();
    assert_eq!(
        view.inserted().iter().collect::<Vec<_>>(),
        vec![
            RecordEntry::new(b"quad-a", Some(&id5)),
            RecordEntry::new(b"quad-b", Some(b"")),
        ]
    );
    assert_eq!(view.removed().len(), 1);
    assert_eq!(
        view.statement_records().iter().collect::<Vec<_>>(),
        vec![
            RecordEntry::new(b"g+stmt-5", Some(b"record-5")),
            RecordEntry::new(b"g+stmt-6", None),
        ]
    );
    assert_eq!(view.operation_count(), 0);
    assert_eq!(view.operation(0), None);
    assert_eq!(view.registration_count(), 1);
    assert_eq!(
        view.registration(0),
        Some(registration(sid(1, 9, 0x40)?, 1, 3)?)
    );
    assert_eq!(view.registration(1), None);
    assert_eq!(view.commit_record().len(), 4_000);
    assert_eq!(
        view.conflict_points().iter().collect::<Vec<_>>(),
        vec![RecordEntry::new(b"\x10key-a", Some(b""))]
    );
    assert_eq!(view.conflict_statements().len(), 1);
    assert_eq!(
        view.conflict_ranges().iter().next(),
        Some(RecordEntry::new(b"\x30a", Some(b"\x30b")))
    );
    assert_eq!(view.result_count(), 3);
    assert_eq!(view.result(0), Some(OperationResult::for_node(0, true)));
    assert_eq!(view.result(1), Some(OperationResult::counter(-5)));
    assert_eq!(view.result(2), Some(OperationResult::compare_and_set()));
    assert_eq!(view.result(3), None);
    assert_eq!(view.result_terms().len(), 1);
    assert_eq!(view.to_record(), record0()?);
    Ok(())
}

#[test]
fn test_the_golden_registration_build_and_drop_read_their_values() -> TestResult {
    let records = golden_records()?;
    let home = WriteSetRecordView::new(&records[1])?;
    assert_eq!(home.kind(), RecordKind::Registration);
    assert_eq!((home.namespace(), home.catalog()), (3, 0));
    assert_eq!(
        home.registration(0),
        Some(registration(sid(1, 12, 0x80)?, 2, 1)?)
    );
    assert_eq!(home.to_record(), record1()?);
    let build = WriteSetRecordView::new(&records[2])?;
    assert_eq!(build.kind(), RecordKind::Build);
    assert_eq!((build.namespace(), build.catalog()), (4, 12));
    assert_eq!(build.operation(0), Some(RecordOperation::StartBuild(12)));
    assert_eq!(build.to_record(), record2());
    let drop = WriteSetRecordView::new(&records[3])?;
    assert_eq!(drop.kind(), RecordKind::Transaction);
    assert_eq!(drop.read_version(), 0);
    assert_eq!(drop.request(), 8);
    assert_eq!(
        drop.operation(0),
        Some(RecordOperation::DropPredicate(sid(1, 4, 0x2A)?))
    );
    assert_eq!(drop.operation_count(), 1);
    assert_eq!(drop.to_record(), record3()?);
    Ok(())
}

#[test]
fn test_a_record_is_checked_as_a_body_with_a_fixed_part() -> TestResult {
    let bytes = golden_records()?[1].clone();
    // 64 bytes of fixed part and 12 × 8 of table: 160.
    assert_eq!(
        refusal(&bytes[..100])?,
        ProtocolErrorEnum::BodyTooShort {
            command: ROUND,
            len: 100,
            need: 160
        }
    );
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(
        refusal(&trailing)?,
        ProtocolErrorEnum::TrailingBytes {
            command: ROUND,
            end: bytes.len(),
            len: bytes.len() + 1
        }
    );
    Ok(())
}

#[test]
fn test_the_kind_and_its_zero_bytes_are_checked() -> TestResult {
    for kind in [0u8, 4, 255] {
        let bytes = changed(0, |fixed, _| fixed[0] = kind)?;
        assert_eq!(
            refusal(&bytes)?,
            ProtocolErrorEnum::FieldValue {
                command: ROUND,
                field: "record kind",
                value: kind.into()
            }
        );
    }
    for at in [1usize, 7] {
        let bytes = changed(0, |fixed, _| fixed[at] = 1)?;
        assert_eq!(
            refusal(&bytes)?,
            ProtocolErrorEnum::PaddingNotZero {
                command: ROUND,
                offset: at
            }
        );
    }
    Ok(())
}

#[test]
fn test_the_fixed_fields_a_kind_leaves_zero_are_zero() -> TestResult {
    // A registration (record 1) leaves every field but the namespace zero; a build (record 2) all but namespace and
    // catalog. The refusal names the field's offset in the fixed part.
    for (index, at) in [
        (1usize, 16usize),
        (1, 24),
        (1, 32),
        (1, 47),
        (1, 48),
        (1, 56),
        (2, 24),
        (2, 40),
        (2, 48),
        (2, 56),
    ] {
        let bytes = changed(index, |fixed, _| fixed[at] = 1)?;
        let field = [16usize, 24, 32, 48, 56]
            .into_iter()
            .rev()
            .find(|start| *start <= at)
            .context("a field's start")?;
        assert_eq!(
            refusal(&bytes)?,
            ProtocolErrorEnum::FieldValue {
                command: ROUND,
                field: "record",
                value: field as u64
            },
            "record {index}, byte {at}"
        );
    }
    // A transaction sets them all; a Drop leaves its read version zero, which the sections' check names.
    let drop = changed(3, |fixed, _| put(fixed, 24, 5, 8))?;
    assert_eq!(
        refusal(&drop)?,
        ProtocolErrorEnum::FieldValue {
            command: ROUND,
            field: "record",
            value: 24
        }
    );
    Ok(())
}

#[test]
fn test_key_value_entries_lie_inside_their_sections() -> TestResult {
    // Section 0's first entry claims a key past the section.
    let past = changed(0, |_, secs| put(&mut secs[0], 0, 4_096, 4))?;
    assert_eq!(
        refusal(&past)?,
        ProtocolErrorEnum::EntryOutOfBounds {
            command: ROUND,
            section: 0,
            entry: 0,
            offset: 0
        }
    );
    // Section 8's entry, `\x30a` and `\x30b`, ends at 12 within its 16 bytes: byte 15 is padding.
    let padded = changed(0, |_, secs| secs[8][15] = 1)?;
    let (start, _) = section(&padded, 64, 8);
    assert_eq!(
        refusal(&padded)?,
        ProtocolErrorEnum::PaddingNotZero {
            command: ROUND,
            offset: start + 15
        }
    );
    Ok(())
}

#[test]
fn test_a_statements_value_is_its_id_or_empty() -> TestResult {
    for (section_index, value, found) in [
        (0usize, Some(&[7u8; 15][..]), 15u64),
        (0, Some(&[7u8; 17][..]), 17),
        (1, None, u64::from(ABSENT)),
    ] {
        let bytes = changed(0, |_, secs| secs[section_index] = entry(b"quad-x", value))?;
        assert_eq!(
            refusal(&bytes)?,
            ProtocolErrorEnum::FieldValue {
                command: ROUND,
                field: "statement id",
                value: found
            }
        );
    }
    // Sixteen bytes and none are both taken, whatever the sixteen bytes.
    let taken = changed(0, |_, secs| {
        secs[1] = [entry(b"q1", Some(&[0; 16])), entry(b"q2", Some(b""))].concat();
    })?;
    assert_eq!(WriteSetRecordView::new(&taken)?.removed().len(), 2);
    Ok(())
}

#[test]
fn test_conflict_entries_hold_the_values_their_section_takes() -> TestResult {
    for (value, found) in [(Some(&b"x"[..]), 1u64), (None, u64::from(ABSENT))] {
        let bytes = changed(0, |_, secs| secs[6] = entry(b"k", value))?;
        assert_eq!(
            refusal(&bytes)?,
            ProtocolErrorEnum::FieldValue {
                command: ROUND,
                field: "conflict point",
                value: found
            }
        );
    }
    for (section_index, field) in [(7usize, "conflict statement"), (8, "conflict range")] {
        let bytes = changed(0, |_, secs| secs[section_index] = entry(b"k", None))?;
        assert_eq!(
            refusal(&bytes)?,
            ProtocolErrorEnum::FieldValue {
                command: ROUND,
                field,
                value: u64::from(ABSENT)
            }
        );
    }
    Ok(())
}

#[test]
fn test_operations_are_whole_known_and_padded_with_zeros() -> TestResult {
    let short = changed(3, |_, secs| {
        let _last: Option<u8> = secs[3].pop();
    })?;
    assert_eq!(
        refusal(&short)?,
        ProtocolErrorEnum::SectionLength {
            command: ROUND,
            section: 3,
            len: 23,
            unit: 24
        }
    );
    for code in [0u64, 8, 0xFFFF] {
        let bytes = changed(3, |_, secs| put(&mut secs[3], 0, code, 2))?;
        assert_eq!(
            refusal(&bytes)?,
            ProtocolErrorEnum::FieldValue {
                command: ROUND,
                field: "operation",
                value: code
            }
        );
    }
    for j in [2usize, 7] {
        let bytes = changed(3, |_, secs| secs[3][j] = 1)?;
        let (start, _) = section(&bytes, 64, 3);
        assert_eq!(
            refusal(&bytes)?,
            ProtocolErrorEnum::PaddingNotZero {
                command: ROUND,
                offset: start + j
            }
        );
    }
    // The dropped predicate's ID with its sequence zeroed is no system ID: the argument names operation 0.
    let bad_id = changed(3, |_, secs| secs[3][10..18].fill(0))?;
    assert_eq!(
        refusal(&bad_id)?,
        ProtocolErrorEnum::FieldValue {
            command: ROUND,
            field: "argument",
            value: 0
        }
    );
    // A build's catalog version holds bytes 8 to 15 of the argument zero.
    let bad_build = changed(2, |_, secs| secs[3][23] = 1)?;
    assert_eq!(
        refusal(&bad_build)?,
        ProtocolErrorEnum::FieldValue {
            command: ROUND,
            field: "argument",
            value: 0
        }
    );
    Ok(())
}

#[test]
fn test_registrations_are_whole_and_valid() -> TestResult {
    let short = changed(1, |_, secs| secs[4].truncate(31))?;
    assert_eq!(
        refusal(&short)?,
        ProtocolErrorEnum::SectionLength {
            command: ROUND,
            section: 4,
            len: 31,
            unit: 32
        }
    );
    // Each break of the registration's rules names registration 0: an entity of type prefix 0, of reserved bytes not
    // zero, a group of 2^18, bytes 20 to 23 not zero, a generation of 0.
    let breaks: [fn(&mut Vec<u8>); 5] = [
        |r| r[0..2].fill(0),
        |r| r[15] = 1,
        |r| put(r, 16, 1 << 18, 4),
        |r| r[22] = 1,
        |r| put(r, 24, 0, 8),
    ];
    for change in breaks {
        let bytes = changed(1, |_, secs| change(&mut secs[4]))?;
        assert_eq!(
            refusal(&bytes)?,
            ProtocolErrorEnum::FieldValue {
                command: ROUND,
                field: "registration",
                value: 0
            }
        );
    }
    // The largest group, 2^18 − 1, is taken.
    let largest = changed(1, |_, secs| put(&mut secs[4], 16, (1 << 18) - 1, 4))?;
    assert_eq!(
        WriteSetRecordView::new(&largest)?
            .registration(0)
            .context("registration 0")?
            .group()
            .get(),
        (1 << 18) - 1
    );
    Ok(())
}

#[test]
fn test_the_results_term_table_holds_iris_alone() -> TestResult {
    // The node's entry turned into a literal of the same text (kind 3).
    let literal = changed(0, |_, secs| secs[10][0] = 3)?;
    assert_eq!(
        refusal(&literal)?,
        ProtocolErrorEnum::FieldValue {
            command: ROUND,
            field: "node term",
            value: 3
        }
    );
    // The table's own rules hold: its texts fill the text section.
    let gap = changed(0, |_, secs| secs[11].push(b'!'))?;
    assert_eq!(
        refusal(&gap)?,
        ProtocolErrorEnum::CountMismatch {
            command: ROUND,
            field: "term text",
            count: 16,
            expected: 15
        }
    );
    Ok(())
}

#[test]
fn test_results_are_checked_as_a_commit_replys() -> TestResult {
    let short = changed(0, |_, secs| secs[9].truncate(47))?;
    assert_eq!(
        refusal(&short)?,
        ProtocolErrorEnum::SectionLength {
            command: ROUND,
            section: 9,
            len: 47,
            unit: 16
        }
    );
    let padded = changed(0, |_, secs| secs[9][16 + 15] = 1)?;
    let (start, _) = section(&padded, 64, 9);
    assert_eq!(
        refusal(&padded)?,
        ProtocolErrorEnum::PaddingNotZero {
            command: ROUND,
            offset: start + 16 + 15
        }
    );
    let created = changed(0, |_, secs| secs[9][12] = 2)?;
    assert_eq!(
        refusal(&created)?,
        ProtocolErrorEnum::FieldValue {
            command: ROUND,
            field: "created",
            value: 2
        }
    );
    // Result 0 names node 1 of a table of one.
    let past = changed(0, |_, secs| put(&mut secs[9], 8, 1, 4))?;
    assert_eq!(
        refusal(&past)?,
        ProtocolErrorEnum::IndexOutOfRange {
            command: ROUND,
            section: 9,
            at: 0,
            index: 1,
            limit: 1
        }
    );
    // A value beside a node, and a creation without one.
    let value = changed(0, |_, secs| put(&mut secs[9], 0, 4, 8))?;
    assert_eq!(
        refusal(&value)?,
        ProtocolErrorEnum::FieldValue {
            command: ROUND,
            field: "result",
            value: 4
        }
    );
    let creation = changed(0, |_, secs| secs[9][32 + 12] = 1)?;
    assert_eq!(
        refusal(&creation)?,
        ProtocolErrorEnum::FieldValue {
            command: ROUND,
            field: "result",
            value: 1
        }
    );
    Ok(())
}

#[test]
fn test_each_kind_holds_its_sections() -> TestResult {
    let records_sections = |count: u64, expected: u64| ProtocolErrorEnum::CountMismatch {
        command: ROUND,
        field: "record sections",
        count,
        expected,
    };
    let golden = golden_records()?;
    let drop_op: Vec<u8> = split_sections(&golden[3], 64, 12)[3].clone();
    let registration_bytes: Vec<u8> = split_sections(&golden[1], 64, 12)[4].clone();
    let build_op: Vec<u8> = split_sections(&golden[2], 64, 12)[3].clone();
    // A transaction holds no operation; one is a Drop, which holds no more than one.
    let two_drops = changed(3, |_, secs| {
        secs[3] = [drop_op.clone(), drop_op.clone()].concat()
    })?;
    assert_eq!(refusal(&two_drops)?, records_sections(2, 1));
    // A Drop holds nothing but its drop and its key ranges: here a statement inserted.
    let drop_inserting = changed(3, |_, secs| secs[0] = entry(b"quad-x", Some(b"")))?;
    assert_eq!(refusal(&drop_inserting)?, records_sections(1, 0));
    // A transaction's operation is a drop, never a build's.
    let tx_build = changed(3, |_, secs| secs[3] = build_op.clone())?;
    assert_eq!(
        refusal(&tx_build)?,
        ProtocolErrorEnum::FieldValue {
            command: ROUND,
            field: "operation",
            value: 6
        }
    );
    // A transaction holds no operation beside its writes.
    let tx_drop = changed(0, |_, secs| secs[3] = drop_op.clone())?;
    assert_eq!(refusal(&tx_drop)?, records_sections(2, 0));
    // A registration holds exactly one registration and nothing else.
    let none = changed(1, |_, secs| secs[4].clear())?;
    assert_eq!(refusal(&none)?, records_sections(0, 1));
    let two = changed(1, |_, secs| secs[4] = registration_bytes.repeat(2))?;
    assert_eq!(refusal(&two)?, records_sections(2, 1));
    let committed = changed(1, |_, secs| secs[5] = b"cr".to_vec())?;
    assert_eq!(refusal(&committed)?, records_sections(2, 0));
    // A build holds exactly one operation, a build's, and nothing else.
    let no_op = changed(2, |_, secs| secs[3].clear())?;
    assert_eq!(refusal(&no_op)?, records_sections(0, 1));
    let build_drop = changed(2, |_, secs| secs[3] = drop_op.clone())?;
    assert_eq!(
        refusal(&build_drop)?,
        ProtocolErrorEnum::FieldValue {
            command: ROUND,
            field: "operation",
            value: 1
        }
    );
    let build_registering = changed(2, |_, secs| secs[4] = registration_bytes.clone())?;
    assert_eq!(refusal(&build_registering)?, records_sections(1, 0));
    let build_range = changed(2, |_, secs| secs[8] = entry(b"a", Some(b"b")))?;
    assert_eq!(refusal(&build_range)?, records_sections(1, 0));
    // A build answers no result and names no node: a counter's result, counted as one result; the transaction's
    // table of one IRI of 15 bytes, counted as one term.
    let counter = [
        (-5i64).to_le_bytes().to_vec(),
        ABSENT.to_le_bytes().to_vec(),
        vec![0; 4],
    ]
    .concat();
    let build_result = changed(2, |_, secs| secs[9] = counter.clone())?;
    assert_eq!(refusal(&build_result)?, records_sections(1, 0));
    let transaction = split_sections(&golden[0], 64, 12);
    let build_terms = changed(2, |_, secs| {
        secs[10] = transaction[10].clone();
        secs[11] = transaction[11].clone();
    })?;
    assert_eq!(refusal(&build_terms)?, records_sections(1, 0));
    Ok(())
}
