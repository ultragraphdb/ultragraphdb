/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, Decision, ErrorCode, RoundView};

use crate::types::round_body::round_body_tests::golden_round;
use crate::types::write_set_record::write_set_record_tests::record0;
use crate::utils::golden_bodies::{
    body_data, frame_of, golden, join_sections, put, reframe, section, split_sections,
    with_command_header, with_section,
};

#[test]
fn test_the_golden_round_reads_its_decisions_and_records() {
    let bytes = golden("round").bytes();
    let view = RoundView::from_frame(&frame_of(&bytes)).unwrap();
    assert_eq!(view.header().position(), 3);
    assert_eq!(view.header().transactions(), 3);
    assert_eq!(
        view.decisions(),
        &[
            Decision::Committed,
            Decision::Aborted {
                code: ErrorCode::Conflict,
                detail: 1
            },
            Decision::Committed
        ]
    );
    assert_eq!(view.records().len(), 2);
    assert_eq!(view.records()[0].to_record(), record0());
    // The committed transaction k has version first_version + 2k.
    assert_eq!(view.commit_version(0), Some(2000));
    assert_eq!(view.commit_version(1), Some(2002));
    assert_eq!(view.commit_version(2), None);
    assert_eq!(view.to_body().unwrap(), golden_round());
}

#[test]
fn test_a_damaged_range_is_named_by_the_transactions_it_holds() {
    let bytes = golden("round").bytes();
    let data = body_data(&bytes).to_vec();
    let (records, _) = section(&data, 0, 2);
    // Record 0 is transaction 0, 288 bytes at `records`; record 1 is transaction 2, from records + 288.
    assert_eq!(
        RoundView::transactions_overlapping(&data, records..records + 1),
        vec![0]
    );
    assert_eq!(
        RoundView::transactions_overlapping(&data, records + 287..records + 289),
        vec![0, 2]
    );
    assert_eq!(
        RoundView::transactions_overlapping(&data, 3_840..data.len()),
        vec![2]
    );
    assert_eq!(
        RoundView::transactions_overlapping(&data, 0..records),
        Vec::<u32>::new()
    );
    assert_eq!(
        RoundView::transactions_overlapping(&data, 0..data.len()),
        vec![0, 2]
    );
    // The body need not pass its checks: a record's bytes damaged.
    let mut damaged = data.clone();
    damaged[4_000] ^= 0xFF;
    assert_eq!(
        RoundView::transactions_overlapping(&damaged, 3_840..7_936),
        vec![2]
    );
    assert_eq!(
        RoundView::transactions_overlapping(&data[..10], 0..10),
        Vec::<u32>::new()
    );
    assert_eq!(
        RoundView::transactions_overlapping(&[], 0..10),
        Vec::<u32>::new()
    );
}

fn refused(index: usize, change: impl FnOnce(&mut Vec<u8>)) -> ProtocolErrorEnum {
    let bytes = with_section(&golden("round").bytes(), 3, index, change);
    RoundView::from_frame(&frame_of(&bytes)).unwrap_err().0
}

#[test]
fn test_decisions_are_counted_and_well_formed() {
    assert_eq!(
        refused(0, |d| d.truncate(32)),
        ProtocolErrorEnum::CountMismatch {
            command: 17,
            field: "decisions",
            count: 2,
            expected: 3
        }
    );
    assert_eq!(
        refused(0, |d| put(d, 16, 99, 2)),
        ProtocolErrorEnum::FieldValue {
            command: 17,
            field: "outcome",
            value: 99
        }
    );
    assert_eq!(
        refused(0, |d| put(d, 2, 1, 2)),
        ProtocolErrorEnum::PaddingNotZero {
            command: 17,
            offset: 24 + 2
        }
    );
    // A commit numbers its record in order and has no detail; an abort names no record.
    assert_eq!(
        refused(0, |d| put(d, 4, 1, 4)),
        ProtocolErrorEnum::FieldValue {
            command: 17,
            field: "record",
            value: 1
        }
    );
    assert_eq!(
        refused(0, |d| put(d, 8, 5, 8)),
        ProtocolErrorEnum::FieldValue {
            command: 17,
            field: "detail",
            value: 5
        }
    );
    assert_eq!(
        refused(0, |d| put(d, 16 + 4, 0, 4)),
        ProtocolErrorEnum::FieldValue {
            command: 17,
            field: "record",
            value: 0
        }
    );
}

#[test]
fn test_the_record_table_lays_records_end_to_end() {
    assert_eq!(
        refused(1, |t| t.truncate(8)),
        ProtocolErrorEnum::CountMismatch {
            command: 17,
            field: "records",
            count: 1,
            expected: 2
        }
    );
    assert_eq!(
        refused(1, |t| put(t, 8, 224, 4)),
        ProtocolErrorEnum::FieldValue {
            command: 17,
            field: "record offset",
            value: 224
        }
    );
    assert_eq!(
        refused(1, |t| put(t, 12, 6_000, 4)),
        ProtocolErrorEnum::IndexOutOfRange {
            command: 17,
            section: 1,
            at: 1,
            index: 6_288,
            limit: 5_432
        }
    );
    // Record 1, 5,144 bytes at 288, one byte longer: one past section 2's 5,432 bytes.
    assert_eq!(
        refused(1, |t| put(t, 12, 5_145, 4)),
        ProtocolErrorEnum::IndexOutOfRange {
            command: 17,
            section: 1,
            at: 1,
            index: 5_433,
            limit: 5_432
        }
    );
    // Record 1 eight bytes past where record 0 ends, eight zero bytes before it: a gap, not the expected offset 288.
    let bytes = golden("round").bytes();
    let mut secs = split_sections(body_data(&bytes), 0, 3);
    put(&mut secs[1], 8, 296, 4);
    secs[2].splice(288..288, [0u8; 8]);
    let gapped = reframe(&bytes, &join_sections(&[], &secs));
    assert_eq!(
        RoundView::from_frame(&frame_of(&gapped)).unwrap_err().0,
        ProtocolErrorEnum::FieldValue {
            command: 17,
            field: "record offset",
            value: 296
        }
    );
    assert_eq!(
        refused(2, |r| r.push(0)),
        ProtocolErrorEnum::TrailingBytes {
            command: 17,
            end: 5_520,
            len: 5_521
        }
    );
}

#[test]
fn test_a_bad_record_is_named_by_its_index() {
    // Record 1's commit record section offset broken: its section 5 table entry, at 40 + 40. The record's own
    // refusal is wrapped: section 5 at offset 1, not where the record's layout puts it, the offset the golden record
    // states there.
    let golden_bytes = golden("round").bytes();
    let records = split_sections(body_data(&golden_bytes), 0, 3)[2].clone();
    let at = 288 + 40 + 40;
    let expected = u32::from_le_bytes(records[at..at + 4].try_into().unwrap()) as usize;
    let bytes = with_section(&golden_bytes, 3, 2, |r| put(r, at, 1, 4));
    assert_eq!(
        RoundView::from_frame(&frame_of(&bytes)).unwrap_err().0,
        ProtocolErrorEnum::Record {
            index: 1,
            cause: Box::new(ProtocolErrorEnum::SectionOffset {
                command: 17,
                section: 5,
                offset: 1,
                expected
            })
        }
    );
    // A round header's reserved bytes, 36 to 128, are zero.
    let reserved = with_command_header(&golden("round").bytes(), |h| h[36] = 1);
    assert_eq!(
        RoundView::from_frame(&frame_of(&reserved)).unwrap_err().0,
        ProtocolErrorEnum::ReservedNotZero { offset: 128 + 36 }
    );
}

/// A round read from its body and header alone, as the log hands one back, is the round its frame holds; a damaged
/// body is refused as the frame's is (7.9).
#[test]
fn test_a_round_read_from_its_body_is_its_frames() {
    let bytes = golden("round").bytes();
    let frame = frame_of(&bytes);
    let view = RoundView::from_frame(&frame).unwrap();
    let data = body_data(&bytes).to_vec();
    let from_body = RoundView::from_body(*view.header(), &data).unwrap();
    assert_eq!(from_body, view);
    let mut short = data.clone();
    short.truncate(16);
    // Three sections take a 24-byte table (PROTOCOL.md section 4.1).
    assert_eq!(
        RoundView::from_body(*view.header(), &short).unwrap_err().0,
        ProtocolErrorEnum::BodyTooShort {
            command: 17,
            len: 16,
            need: 24
        }
    );
}
