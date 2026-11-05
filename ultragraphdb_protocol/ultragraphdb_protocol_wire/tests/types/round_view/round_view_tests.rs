/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    ABSENT, BodyView, Decision, ErrorCode, RecordEntry, RecordKind, RoundView,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::round_body::round_body_tests::golden_round;
use crate::types::write_set_record::write_set_record_tests::{FIRST_VERSION, record0};
use crate::utils::golden_bodies::{
    body_data, frame_of, golden, join_sections, put, reframe, section, split_sections,
    with_command_header, with_section,
};

/// The Round command.
const ROUND: u16 = 17;

/// Where the golden body's sections start, as generate_bodies.py printed and its layout gives: decisions at 32 (80
/// bytes), the record table at 112 (32), the records at 144 (5,040), the dictionary at 5,184 (64).
const DECISIONS: usize = 32;
const RECORDS: usize = 144;
const DICTIONARY: usize = 5_184;
/// The records' offsets within section 2 and their lengths, as generate_bodies.py printed them.
const RECORD_AT: [usize; 4] = [0, 4_456, 4_648, 4_832];
const RECORD_LEN: [usize; 4] = [4_455, 192, 184, 208];

fn refused(index: usize, change: impl FnOnce(&mut Vec<u8>)) -> TestResult<ProtocolErrorEnum> {
    let bytes = with_section(&golden("round")?.bytes()?, 4, index, change);
    Ok(RoundView::from_frame(&frame_of(&bytes)?)
        .err()
        .context("a refusal")?
        .0)
}

fn refused_header(change: impl FnOnce(&mut [u8])) -> TestResult<ProtocolErrorEnum> {
    let bytes = with_command_header(&golden("round")?.bytes()?, change);
    Ok(RoundView::from_frame(&frame_of(&bytes)?)
        .err()
        .context("a refusal")?
        .0)
}

/// A dictionary entry laid out from the layout table.
fn entry(key: &[u8], value: Option<&[u8]>) -> Vec<u8> {
    let mut out = (key.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(&value.map_or(ABSENT, |v| v.len() as u32).to_le_bytes());
    out.extend_from_slice(key);
    out.extend_from_slice(value.unwrap_or_default());
    out.resize(out.len().div_ceil(8) * 8, 0);
    out
}

#[test]
fn test_the_golden_round_reads_its_decisions_records_and_dictionary() -> TestResult {
    let bytes = golden("round")?.bytes()?;
    let view = RoundView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(view.header().position(), 3);
    assert_eq!(view.header().transactions(), 5);
    assert_eq!(view.header().derivation().get(), 1);
    assert_eq!(view.header().first_version(), FIRST_VERSION);
    assert_eq!(
        view.decisions(),
        &[
            Decision::Committed,
            Decision::Aborted {
                code: ErrorCode::Conflict,
                detail: 1
            },
            Decision::Committed,
            Decision::Committed,
            Decision::Committed,
        ]
    );
    assert_eq!(
        view.records().iter().map(|r| r.kind()).collect::<Vec<_>>(),
        vec![
            RecordKind::Transaction,
            RecordKind::Registration,
            RecordKind::Build,
            RecordKind::Transaction
        ]
    );
    assert_eq!(view.records()[0].to_record(), record0()?);
    // Committed entry k has version first_version + 2k.
    assert_eq!(view.commit_version(0), Some(FIRST_VERSION));
    assert_eq!(view.commit_version(3), Some(FIRST_VERSION + 6));
    assert_eq!(view.commit_version(4), None);
    assert_eq!(
        view.dictionary().iter().collect::<Vec<_>>(),
        vec![
            RecordEntry::new(b"\x20dict-a", Some(b"value-a")),
            RecordEntry::new(b"\x20dict-b", Some(b"")),
            RecordEntry::new(b"\x21dict-c", Some(b"value-c")),
        ]
    );
    assert_eq!(view.to_body()?, golden_round()?);
    Ok(())
}

#[test]
fn test_a_damaged_range_is_named_by_the_entries_it_holds() -> TestResult {
    let bytes = golden("round")?.bytes()?;
    let data = body_data(&bytes).to_vec();
    assert_eq!(section(&data, 0, 2).0, RECORDS);
    let start = |i: usize| RECORDS + RECORD_AT[i];
    // Record 0 is entry 0, record 1 entry 2, record 2 entry 3, record 3 entry 4; entry 1 aborted.
    assert_eq!(
        RoundView::transactions_overlapping(&data, start(0)..start(0) + 1),
        vec![0]
    );
    let end0 = start(0) + RECORD_LEN[0];
    assert_eq!(
        RoundView::transactions_overlapping(&data, end0 - 1..end0 + 2),
        vec![0, 2]
    );
    assert_eq!(
        RoundView::transactions_overlapping(&data, start(2)..start(2) + 1),
        vec![3]
    );
    assert_eq!(
        RoundView::transactions_overlapping(&data, 3_840..data.len()),
        vec![0, 2, 3, 4]
    );
    assert_eq!(
        RoundView::transactions_overlapping(&data, 0..RECORDS),
        Vec::<u32>::new()
    );
    assert_eq!(
        RoundView::transactions_overlapping(&data, DICTIONARY..data.len()),
        Vec::<u32>::new()
    );
    // The body need not pass its checks: a record's bytes damaged.
    let mut damaged = data.clone();
    damaged[start(3) + 1] ^= 0xFF;
    assert_eq!(
        RoundView::transactions_overlapping(&damaged, start(3)..start(3) + 8),
        vec![4]
    );
    assert_eq!(
        RoundView::transactions_overlapping(&data[..10], 0..10),
        Vec::<u32>::new()
    );
    assert_eq!(
        RoundView::transactions_overlapping(&[], 0..10),
        Vec::<u32>::new()
    );
    Ok(())
}

#[test]
fn test_decisions_are_counted_and_well_formed() -> TestResult {
    assert_eq!(
        refused(0, |d| d.truncate(64))?,
        ProtocolErrorEnum::CountMismatch {
            command: ROUND,
            field: "decisions",
            count: 4,
            expected: 5
        }
    );
    assert_eq!(
        refused(0, |d| d.truncate(70))?,
        ProtocolErrorEnum::SectionLength {
            command: ROUND,
            section: 0,
            len: 70,
            unit: 16
        }
    );
    for outcome in [99u64, 0xFFFF] {
        assert_eq!(
            refused(0, |d| put(d, 16, outcome, 2))?,
            ProtocolErrorEnum::FieldValue {
                command: ROUND,
                field: "outcome",
                value: outcome
            }
        );
    }
    assert_eq!(
        refused(0, |d| put(d, 2, 1, 2))?,
        ProtocolErrorEnum::PaddingNotZero {
            command: ROUND,
            offset: DECISIONS + 2
        }
    );
    // The second zero byte of decision 1 names its own offset.
    assert_eq!(
        refused(0, |d| d[16 + 3] = 1)?,
        ProtocolErrorEnum::PaddingNotZero {
            command: ROUND,
            offset: DECISIONS + 16 + 3
        }
    );
    Ok(())
}

#[test]
fn test_a_commit_numbers_its_record_and_an_abort_names_none() -> TestResult {
    // A commit numbers its record in order and has no detail; an abort names no record.
    assert_eq!(
        refused(0, |d| put(d, 4, 1, 4))?,
        ProtocolErrorEnum::FieldValue {
            command: ROUND,
            field: "record",
            value: 1
        }
    );
    assert_eq!(
        refused(0, |d| put(d, 32 + 4, 2, 4))?,
        ProtocolErrorEnum::FieldValue {
            command: ROUND,
            field: "record",
            value: 2
        }
    );
    assert_eq!(
        refused(0, |d| put(d, 8, 5, 8))?,
        ProtocolErrorEnum::FieldValue {
            command: ROUND,
            field: "detail",
            value: 5
        }
    );
    assert_eq!(
        refused(0, |d| put(d, 16 + 4, 0, 4))?,
        ProtocolErrorEnum::FieldValue {
            command: ROUND,
            field: "record",
            value: 0
        }
    );
    Ok(())
}

#[test]
fn test_the_last_commit_stays_in_the_first_versions_epoch() -> TestResult {
    // Four commits: the last at first_version + 6. A sequence of 2^22 − 8 puts it at 2^22 − 2, the last of the epoch.
    let at_bound = (7u64 << 22) | ((1 << 22) - 8);
    let bytes = with_command_header(&golden("round")?.bytes()?, |h| put(h, 24, at_bound, 8));
    let view = RoundView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(view.commit_version(3), Some((7 << 22) | ((1 << 22) - 2)));
    // One further puts the last at sequence 2^22 − 1, which no version takes; two further, into epoch 8.
    for first in [at_bound + 1, at_bound + 2] {
        assert_eq!(
            refused_header(|h| put(h, 24, first, 8))?,
            ProtocolErrorEnum::FieldValue {
                command: ROUND,
                field: "first_version",
                value: first
            }
        );
    }
    Ok(())
}

#[test]
fn test_the_header_holds_its_unused_bytes_zero_and_a_derivation() -> TestResult {
    assert_eq!(
        refused_header(|h| h[40] = 1)?,
        ProtocolErrorEnum::ReservedNotZero { offset: 128 + 40 }
    );
    assert_eq!(
        refused_header(|h| put(h, 36, 0, 4))?,
        ProtocolErrorEnum::FieldValue {
            command: ROUND,
            field: "derivation",
            value: 0
        }
    );
    // Any derivation from 1 reads: the reader of the record decides which it knows.
    let bytes = with_command_header(&golden("round")?.bytes()?, |h| {
        put(h, 36, u32::MAX.into(), 4)
    });
    assert_eq!(
        RoundView::from_frame(&frame_of(&bytes)?)?
            .header()
            .derivation()
            .get(),
        u32::MAX
    );
    Ok(())
}

#[test]
fn test_the_record_table_lays_records_end_to_end() -> TestResult {
    assert_eq!(
        refused(1, |t| t.truncate(24))?,
        ProtocolErrorEnum::CountMismatch {
            command: ROUND,
            field: "records",
            count: 3,
            expected: 4
        }
    );
    assert_eq!(
        refused(1, |t| t.truncate(20))?,
        ProtocolErrorEnum::SectionLength {
            command: ROUND,
            section: 1,
            len: 20,
            unit: 8
        }
    );
    assert_eq!(
        refused(1, |t| put(t, 8, 4_448, 4))?,
        ProtocolErrorEnum::FieldValue {
            command: ROUND,
            field: "record offset",
            value: 4_448
        }
    );
    assert_eq!(
        refused(1, |t| put(t, 12, 6_000, 4))?,
        ProtocolErrorEnum::IndexOutOfRange {
            command: ROUND,
            section: 1,
            at: 1,
            index: 4_456 + 6_000,
            limit: 5_040
        }
    );
    // Record 3, 208 bytes at 4,832, one byte longer: one past section 2's 5,040 bytes.
    assert_eq!(
        refused(1, |t| put(t, 28, 209, 4))?,
        ProtocolErrorEnum::IndexOutOfRange {
            command: ROUND,
            section: 1,
            at: 3,
            index: 5_041,
            limit: 5_040
        }
    );
    Ok(())
}

#[test]
fn test_records_lie_end_to_end_with_zero_padding_and_nothing_after() -> TestResult {
    // Record 1 eight bytes past where record 0's padding ends, eight zero bytes before it: not the expected 4,456.
    let bytes = golden("round")?.bytes()?;
    let mut secs = split_sections(body_data(&bytes), 0, 4);
    put(&mut secs[1], 8, 4_464, 4);
    let removed: Vec<u8> = secs[2].splice(4_456..4_456, [0u8; 8]).collect();
    assert!(removed.is_empty());
    let gapped = reframe(&bytes, &join_sections(&[], &secs));
    assert_eq!(
        RoundView::from_frame(&frame_of(&gapped)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::FieldValue {
            command: ROUND,
            field: "record offset",
            value: 4_464
        }
    );
    // The byte between record 0's end and record 1 is padding.
    assert_eq!(
        refused(2, |r| r[RECORD_LEN[0]] = 1)?,
        ProtocolErrorEnum::PaddingNotZero {
            command: ROUND,
            offset: RECORDS + RECORD_LEN[0]
        }
    );
    assert_eq!(
        refused(2, |r| r.push(0))?,
        ProtocolErrorEnum::TrailingBytes {
            command: ROUND,
            end: DICTIONARY,
            len: DICTIONARY + 1
        }
    );
    Ok(())
}

#[test]
fn test_a_bad_record_is_named_by_its_index() -> TestResult {
    assert_eq!(
        refused(2, |r| r[RECORD_AT[1]] = 9)?,
        ProtocolErrorEnum::Record {
            index: 1,
            cause: Box::new(ProtocolErrorEnum::FieldValue {
                command: ROUND,
                field: "record kind",
                value: 9
            })
        }
    );
    Ok(())
}

#[test]
fn test_a_tenants_records_name_one_catalog() -> TestResult {
    // The Drop of tenant 3 at catalog 12, beside the transaction of tenant 3 at catalog 11. The registration of
    // tenant 3, catalog 0, and the build of tenant 4 at catalog 12 name no other catalog of it.
    assert_eq!(
        refused(2, |r| put(r, RECORD_AT[3] + 16, 12, 8))?,
        ProtocolErrorEnum::FieldValue {
            command: ROUND,
            field: "catalog",
            value: 12
        }
    );
    Ok(())
}

#[test]
fn test_dictionary_entries_are_framed_present_and_ascending() -> TestResult {
    assert_eq!(
        refused(3, |s| put(s, 0, 4_096, 4))?,
        ProtocolErrorEnum::EntryOutOfBounds {
            command: ROUND,
            section: 3,
            entry: 0,
            offset: 0
        }
    );
    // Entry 1, `\x20dict-b` and an empty value, is 15 bytes from 24: byte 39 is its padding.
    assert_eq!(
        refused(3, |s| s[39] = 1)?,
        ProtocolErrorEnum::PaddingNotZero {
            command: ROUND,
            offset: DICTIONARY + 39
        }
    );
    assert_eq!(
        refused(3, |s| *s =
            [entry(b"\x20a", Some(b"v")), entry(b"\x20b", None)].concat())?,
        ProtocolErrorEnum::FieldValue {
            command: ROUND,
            field: "dictionary entry",
            value: u64::from(ABSENT)
        }
    );
    let disorders: [[&[u8]; 2]; 2] = [[b"\x20b", b"\x20a"], [b"\x20a", b"\x20a"]];
    for keys in disorders {
        assert_eq!(
            refused(3, |s| *s = keys
                .iter()
                .map(|k| entry(k, Some(b"v")))
                .collect::<Vec<_>>()
                .concat())?,
            ProtocolErrorEnum::FieldValue {
                command: ROUND,
                field: "dictionary order",
                value: 1
            }
        );
    }
    // A shorter key that prefixes a longer one comes first, byte by byte.
    let bytes = with_section(&golden("round")?.bytes()?, 4, 3, |s| {
        *s = [entry(b"\x20a", Some(b"v")), entry(b"\x20ab", Some(b"w"))].concat();
    });
    assert_eq!(
        RoundView::from_frame(&frame_of(&bytes)?)?
            .dictionary()
            .len(),
        2
    );
    Ok(())
}

/// A round read from its body and header alone, as the log hands one back, is the round its frame holds; a damaged
/// body is refused as the frame's is.
#[test]
fn test_a_round_read_from_its_body_is_its_frames() -> TestResult {
    let bytes = golden("round")?.bytes()?;
    let frame = frame_of(&bytes)?;
    let view = RoundView::from_frame(&frame)?;
    let data = body_data(&bytes).to_vec();
    let from_body = RoundView::from_body(*view.header(), &data)?;
    assert_eq!(from_body, view);
    let mut short = data.clone();
    short.truncate(16);
    // Four sections take a 32-byte table (PROTOCOL.md section 4.1).
    assert_eq!(
        RoundView::from_body(*view.header(), &short)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::BodyTooShort {
            command: ROUND,
            len: 16,
            need: 32
        }
    );
    Ok(())
}

#[test]
fn test_padding_after_a_record_is_named_at_its_own_byte() -> TestResult {
    use ultragraphdb_protocol_wire::{BodyBuilder, RoundBody, WriteSetRecord};

    use crate::types::round_body::round_body_tests::first_derivation;
    // A record whose results' term text is 18 bytes ends 2 past a multiple of 8, so 6 bytes of padding follow it.
    let mut record = WriteSetRecord::transaction(1);
    record.push_node_result("http://ex.org/abcd", true);
    let bytes = record.encode();
    assert_eq!(bytes.len() % 8, 2);
    let mut round = RoundBody::new(1, 0, 1, FIRST_VERSION, first_derivation()?);
    round.commit(bytes.clone());
    round.commit(bytes.clone());
    let mut body = round.body();
    let (records, _) = section(&body, 0, 2);
    let padding = records + bytes.len() + 3;
    body[padding] = 1;
    assert_eq!(
        RoundView::from_body(round.header(), &body)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::PaddingNotZero {
            command: ROUND,
            offset: padding
        }
    );
    Ok(())
}
