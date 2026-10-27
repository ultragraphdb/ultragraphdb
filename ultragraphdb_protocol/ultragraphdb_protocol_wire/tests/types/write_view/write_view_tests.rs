/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, Write, WriteView};

use crate::types::write::write_tests::golden_changeset;
use crate::utils::golden_bodies::{frame_of, golden, put, with_section};

fn refused(index: usize, change: impl FnOnce(&mut Vec<u8>)) -> ProtocolErrorEnum {
    let bytes = with_section(&golden("write").bytes(), 4, index, change);
    match WriteView::from_frame(&frame_of(&bytes)) {
        Err(e) => e.0,
        Ok(view) => view.to_body().unwrap_err().0,
    }
}

#[test]
fn test_the_golden_write_reads_its_changeset() {
    let bytes = golden("write").bytes();
    let view = WriteView::from_frame(&frame_of(&bytes)).unwrap();
    assert_eq!(view.namespace(), 2);
    assert_eq!(view.len(), 11);
    assert!(!view.is_empty());
    assert_eq!(view.strings().get(0), Some("<http://ex.org/a>"));
    assert_eq!(view.to_body().unwrap(), Write::new(golden_changeset()));
}

#[test]
fn test_records_are_whole_known_and_zero_where_unused() {
    assert_eq!(
        refused(0, |r| r.truncate(r.len() - 1)),
        ProtocolErrorEnum::SectionLength {
            command: 8,
            section: 0,
            len: 175,
            unit: 16
        }
    );
    assert_eq!(
        refused(0, |r| r[0] = 11),
        ProtocolErrorEnum::FieldValue {
            command: 8,
            field: "change kind",
            value: 11
        }
    );
    assert_eq!(
        refused(0, |r| r[0] = 0),
        ProtocolErrorEnum::FieldValue {
            command: 8,
            field: "change kind",
            value: 0
        }
    );
    // Insert takes no flags; DeleteProperty only bit 0; a predicate declaration's kind bits only 0 to 2.
    assert_eq!(
        refused(0, |r| r[1] = 1),
        ProtocolErrorEnum::FieldValue {
            command: 8,
            field: "flags",
            value: 1
        }
    );
    assert_eq!(
        refused(0, |r| r[16 * 6 + 1] = 3),
        ProtocolErrorEnum::FieldValue {
            command: 8,
            field: "flags",
            value: 3
        }
    );
    assert_eq!(
        refused(0, |r| r[16 * 7 + 1] = 3 | 4),
        ProtocolErrorEnum::FieldValue {
            command: 8,
            field: "flags",
            value: 7
        }
    );
    assert_eq!(
        refused(0, |r| r[16 * 7 + 1] = 64 | 1),
        ProtocolErrorEnum::FieldValue {
            command: 8,
            field: "flags",
            value: 65
        }
    );
    // A change's unused bytes; extra is zero in a Write.
    assert_eq!(
        refused(0, |r| r[2] = 1),
        ProtocolErrorEnum::FieldValue {
            command: 8,
            field: "change record",
            value: 1
        }
    );
    assert_eq!(
        refused(0, |r| r[16 * 9 + 12] = 1),
        ProtocolErrorEnum::FieldValue {
            command: 8,
            field: "extra",
            value: 1
        }
    );
}

#[test]
fn test_args_follow_each_other_and_are_consumed_exactly() {
    // Change 1 starts at arg 4.
    assert_eq!(
        refused(0, |r| put(r, 16 + 4, 5, 4)),
        ProtocolErrorEnum::FieldValue {
            command: 8,
            field: "args_start",
            value: 5
        }
    );
    // Insert takes 4 args.
    assert_eq!(
        refused(0, |r| put(r, 8, 3, 4)),
        ProtocolErrorEnum::CountMismatch {
            command: 8,
            field: "change args",
            count: 3,
            expected: 4
        }
    );
    // One arg more than the records consume.
    assert_eq!(
        refused(1, |a| a.extend_from_slice(&0u32.to_le_bytes())),
        ProtocolErrorEnum::CountMismatch {
            command: 8,
            field: "args",
            count: 54,
            expected: 53
        }
    );
    // A tokenizer count past the change's args.
    assert_eq!(
        refused(1, |a| put(a, 4 * (4 + 4 + 3 + 3 + 6 + 5 + 6 + 2), 40, 4)),
        ProtocolErrorEnum::CountMismatch {
            command: 8,
            field: "change args",
            count: 7,
            expected: 43
        }
    );
}

#[test]
fn test_string_references_are_in_range_and_absent_only_where_optional() {
    // The Insert's subject, arg 1, past the strings; then absent.
    let strings = 27u32;
    assert_eq!(
        refused(1, |a| put(a, 4, strings.into(), 4)),
        ProtocolErrorEnum::IndexOutOfRange {
            command: 8,
            section: 1,
            at: 1,
            index: strings,
            limit: strings
        }
    );
    assert_eq!(
        refused(1, |a| put(a, 4, u32::MAX.into(), 4)),
        ProtocolErrorEnum::IndexOutOfRange {
            command: 8,
            section: 1,
            at: 1,
            index: u32::MAX,
            limit: strings
        }
    );
}

#[test]
fn test_terms_parse_as_their_place_takes_them() {
    // String 0 is "<http://ex.org/a>", the Insert's subject: a literal there, or no term at all, is refused.
    let replace = |with: &str| {
        let with = with.to_string();
        let bytes = golden("write").bytes();
        let view_bytes = {
            let secs = crate::utils::golden_bodies::split_sections(
                crate::utils::golden_bodies::body_data(&bytes),
                0,
                4,
            );
            let old = "<http://ex.org/a>";
            let text = String::from_utf8(secs[3].clone()).unwrap();
            let text = text.replacen(old, &with, 1);
            let delta = with.len() as i64 - old.len() as i64;
            let ends: Vec<u8> = secs[2]
                .chunks(4)
                .map(|c| (u32::from_le_bytes(c.try_into().unwrap()) as i64 + delta) as u32)
                .flat_map(|e| e.to_le_bytes())
                .collect();
            let joined = crate::utils::golden_bodies::join_sections(
                &[],
                &[secs[0].clone(), secs[1].clone(), ends, text.into_bytes()],
            );
            crate::utils::golden_bodies::reframe(&bytes, &joined)
        };
        WriteView::from_frame(&frame_of(&view_bytes))
            .unwrap()
            .to_body()
            .map(|_| ())
    };
    assert_eq!(
        replace("\"lit\"").unwrap_err().0,
        ProtocolErrorEnum::BadTerm {
            command: 8,
            section: 3,
            at: 0
        }
    );
    assert_eq!(
        replace("not a term").unwrap_err().0,
        ProtocolErrorEnum::BadTerm {
            command: 8,
            section: 3,
            at: 0
        }
    );
    assert_eq!(replace("_:z"), Ok(()));
}
