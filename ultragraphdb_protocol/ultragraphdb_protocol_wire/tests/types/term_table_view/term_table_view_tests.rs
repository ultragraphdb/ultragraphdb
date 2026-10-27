/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyView, Direction, ExecuteView, MAX_TERM_DEPTH, RequestTerm, RequestTermView, TermTable,
};

use crate::utils::golden_bodies::{frame_of, golden, join_sections, reframe};

/// A term record: kind, direction, six zero bytes, then the four u32 fields.
pub fn record(kind: u8, direction: u8, fields: [u32; 4]) -> Vec<u8> {
    let mut r = vec![kind, direction, 0, 0, 0, 0, 0, 0];
    fields
        .iter()
        .for_each(|f| r.extend_from_slice(&f.to_le_bytes()));
    r
}

/// The golden Execute's frame around a term table of `records` and `text`, binding no parameter. Its records are the
/// body's section 0, at body offset 32.
pub fn execute(records: &[Vec<u8>], text: &[u8]) -> Vec<u8> {
    let body = join_sections(
        &[],
        &[records.concat(), text.to_vec(), Vec::new(), Vec::new()],
    );
    reframe(&golden("execute").bytes(), &body)
}

fn read(records: &[Vec<u8>], text: &[u8]) -> Result<TermTable, ProtocolErrorEnum> {
    let bytes = execute(records, text);
    ExecuteView::from_frame(&frame_of(&bytes))
        .map(|v| v.terms().to_table())
        .map_err(|e| e.0)
}

fn refused(records: &[Vec<u8>], text: &[u8]) -> ProtocolErrorEnum {
    read(records, text).unwrap_err()
}

#[test]
fn test_every_kind_reads_in_place() {
    let records = [
        record(1, 0, [0, 3, 3, 0]),
        record(2, 0, [3, 2, 5, 0]),
        record(3, 0, [5, 2, 7, 0]),
        record(3, 0, [7, 2, 9, 3]),
        record(4, 2, [12, 2, 14, 2]),
        record(5, 0, [0, 0, 1, 0]),
    ];
    // Texts: "abc" 0..3, "b1" 3..5, "x1" 5..7, "35" 7..9 and "int" 9..12, "hi" 12..14 and "ar" 14..16.
    let bytes = execute(&records, b"abcb1x135inthiar");
    let view = ExecuteView::from_frame(&frame_of(&bytes)).unwrap();
    let terms = view.terms();
    assert_eq!(terms.len(), 6);
    assert!(!terms.is_empty());
    assert_eq!(terms.get(0), Some(RequestTermView::Iri("abc")));
    assert_eq!(terms.get(1), Some(RequestTermView::BlankNode("b1")));
    assert_eq!(
        terms.get(2),
        Some(RequestTermView::Literal {
            lexical: "x1",
            datatype: None
        })
    );
    assert_eq!(
        terms.get(3),
        Some(RequestTermView::Literal {
            lexical: "35",
            datatype: Some("int")
        })
    );
    assert_eq!(
        terms.get(4),
        Some(RequestTermView::LanguageLiteral {
            lexical: "hi",
            language: "ar",
            direction: Some(Direction::Rtl)
        })
    );
    assert_eq!(
        terms.get(5),
        Some(RequestTermView::Triple {
            subject: 0,
            predicate: 0,
            object: 1
        })
    );
    assert_eq!(terms.get(6), None);
    assert_eq!(
        terms.get(4).unwrap().to_term(),
        RequestTerm::LanguageLiteral {
            lexical: "hi".to_string(),
            language: "ar".to_string(),
            direction: Some(Direction::Rtl),
        }
    );
    assert_eq!(
        terms.to_table().terms()[1],
        RequestTerm::BlankNode("b1".to_string())
    );
}

#[test]
fn test_records_are_whole_and_their_fixed_fields_known() {
    let mut short = record(1, 0, [0, 1, 1, 0]);
    short.pop();
    assert_eq!(
        refused(&[short], b"a"),
        ProtocolErrorEnum::SectionLength {
            command: 11,
            section: 0,
            len: 23,
            unit: 24
        }
    );
    for kind in [0u8, 6, 255] {
        assert_eq!(
            refused(&[record(kind, 0, [0, 1, 1, 0])], b"a"),
            ProtocolErrorEnum::FieldValue {
                command: 11,
                field: "term kind",
                value: kind.into()
            }
        );
    }
    assert_eq!(
        refused(&[record(1, 1, [0, 1, 1, 0])], b"a"),
        ProtocolErrorEnum::FieldValue {
            command: 11,
            field: "direction",
            value: 1
        }
    );
    assert_eq!(
        refused(&[record(4, 3, [0, 1, 1, 1])], b"ab"),
        ProtocolErrorEnum::FieldValue {
            command: 11,
            field: "direction",
            value: 3
        }
    );
    let mut reserved = record(1, 0, [0, 1, 1, 0]);
    reserved[5] = 1;
    // The records start at body offset 32, after the table of four sections.
    assert_eq!(
        refused(&[reserved], b"a"),
        ProtocolErrorEnum::PaddingNotZero {
            command: 11,
            offset: 32 + 5
        }
    );
}

#[test]
fn test_texts_follow_each_other_and_fill_their_section() {
    assert_eq!(
        refused(&[record(1, 0, [1, 2, 3, 0])], b"abc"),
        ProtocolErrorEnum::FieldValue {
            command: 11,
            field: "term text offset",
            value: 1
        }
    );
    assert_eq!(
        refused(&[record(3, 0, [0, 2, 1, 1])], b"abc"),
        ProtocolErrorEnum::FieldValue {
            command: 11,
            field: "term text offset",
            value: 1
        }
    );
    assert_eq!(
        refused(&[record(1, 0, [0, 10, 10, 0])], b"abc"),
        ProtocolErrorEnum::IndexOutOfRange {
            command: 11,
            section: 1,
            at: 0,
            index: 10,
            limit: 3
        }
    );
    assert_eq!(
        refused(&[record(1, 0, [0, 3, 3, 0])], b"abcX"),
        ProtocolErrorEnum::CountMismatch {
            command: 11,
            field: "term text",
            count: 4,
            expected: 3
        }
    );
    // An empty table holds no text.
    assert_eq!(
        refused(&[], b"a"),
        ProtocolErrorEnum::CountMismatch {
            command: 11,
            field: "term text",
            count: 1,
            expected: 0
        }
    );
    assert_eq!(read(&[], b""), Ok(TermTable::new()));
}

#[test]
fn test_a_text_one_byte_past_its_section_names_the_entry() {
    // An IRI of two bytes over a text section of one: refused as out of range, before the section's fill is checked.
    assert_eq!(
        refused(&[record(1, 0, [0, 2, 2, 0])], b"a"),
        ProtocolErrorEnum::IndexOutOfRange {
            command: 11,
            section: 1,
            at: 0,
            index: 2,
            limit: 1
        }
    );
}

#[test]
fn test_the_text_is_utf8_and_cut_on_character_boundaries() {
    assert_eq!(
        refused(&[record(1, 0, [0, 2, 2, 0])], &[b'a', 0xFF]),
        ProtocolErrorEnum::NotUtf8 {
            command: 11,
            section: 1,
            offset: 1
        }
    );
    // "ü" is two bytes: an IRI of its first and a blank node of its second, each invalid alone.
    assert_eq!(
        refused(
            &[record(1, 0, [0, 1, 1, 0]), record(2, 0, [1, 1, 2, 0])],
            "ü".as_bytes()
        ),
        ProtocolErrorEnum::NotUtf8 {
            command: 11,
            section: 1,
            offset: 1
        }
    );
}

#[test]
fn test_a_second_text_is_what_the_kind_takes() {
    // An IRI and a blank node have none; a literal never states xsd:string; a language tag is never empty.
    assert_eq!(
        refused(&[record(1, 0, [0, 1, 1, 1])], b"ab"),
        ProtocolErrorEnum::BadTerm {
            command: 11,
            section: 0,
            at: 0
        }
    );
    assert_eq!(
        refused(&[record(2, 0, [0, 1, 1, 1])], b"ab"),
        ProtocolErrorEnum::BadTerm {
            command: 11,
            section: 0,
            at: 0
        }
    );
    let xsd = "http://www.w3.org/2001/XMLSchema#string";
    let text = format!("x{xsd}");
    assert_eq!(
        refused(
            &[record(3, 0, [0, 1, 1, xsd.len() as u32])],
            text.as_bytes()
        ),
        ProtocolErrorEnum::BadTerm {
            command: 11,
            section: 0,
            at: 0
        }
    );
    assert_eq!(
        refused(&[record(4, 0, [0, 1, 1, 0])], b"x"),
        ProtocolErrorEnum::BadTerm {
            command: 11,
            section: 0,
            at: 0
        }
    );
}

#[test]
fn test_a_triple_term_names_earlier_entries_only() {
    // Entry 0 naming itself; entry 1 naming entry 1 or 2.
    assert_eq!(
        refused(&[record(5, 0, [0, 0, 0, 0])], b""),
        ProtocolErrorEnum::IndexOutOfRange {
            command: 11,
            section: 0,
            at: 0,
            index: 0,
            limit: 0
        }
    );
    assert_eq!(
        refused(
            &[record(1, 0, [0, 1, 1, 0]), record(5, 0, [0, 1, 0, 0])],
            b"a"
        ),
        ProtocolErrorEnum::IndexOutOfRange {
            command: 11,
            section: 0,
            at: 1,
            index: 1,
            limit: 1
        }
    );
    assert_eq!(
        refused(
            &[record(1, 0, [0, 1, 1, 0]), record(5, 0, [0, 0, 2, 0])],
            b"a"
        ),
        ProtocolErrorEnum::IndexOutOfRange {
            command: 11,
            section: 0,
            at: 1,
            index: 2,
            limit: 1
        }
    );
    // Its fourth field is zero.
    assert_eq!(
        refused(
            &[record(1, 0, [0, 1, 1, 0]), record(5, 0, [0, 0, 0, 7])],
            b"a"
        ),
        ProtocolErrorEnum::FieldValue {
            command: 11,
            field: "triple term",
            value: 7
        }
    );
}

#[test]
fn test_no_term_appears_twice() {
    assert_eq!(
        refused(
            &[record(1, 0, [0, 1, 1, 0]), record(1, 0, [1, 1, 2, 0])],
            b"aa"
        ),
        ProtocolErrorEnum::DuplicateTerm {
            command: 11,
            section: 0,
            at: 1,
            earlier: 0
        }
    );
    assert_eq!(
        refused(
            &[
                record(1, 0, [0, 1, 1, 0]),
                record(5, 0, [0, 0, 0, 0]),
                record(5, 0, [0, 0, 0, 0])
            ],
            b"a"
        ),
        ProtocolErrorEnum::DuplicateTerm {
            command: 11,
            section: 0,
            at: 2,
            earlier: 1
        }
    );
    // The same text as an IRI and as a blank node is two terms.
    assert!(
        read(
            &[record(1, 0, [0, 1, 1, 0]), record(2, 0, [1, 1, 2, 0])],
            b"aa"
        )
        .is_ok()
    );
}

/// `<a>` and a chain of `depth` triple terms, entry k + 1 naming entry k at the place `at` (0 subject, 2 object), the
/// others `<a>`.
fn chain(depth: usize, at: usize) -> Vec<Vec<u8>> {
    let mut records = vec![record(1, 0, [0, 1, 1, 0])];
    for k in 0..depth as u32 {
        let mut parts = [0u32; 3];
        parts[at] = k;
        // Each triple term differs from the one before, so none repeats: the first names <a> everywhere.
        records.push(record(5, 0, [parts[0], parts[1], parts[2], 0]));
    }
    records
}

#[test]
fn test_triple_terms_nest_at_most_the_maximum_through_any_part() {
    for at in [0, 2] {
        assert!(
            read(&chain(MAX_TERM_DEPTH, at), b"a").is_ok(),
            "depth 32 through part {at}"
        );
        assert_eq!(
            refused(&chain(MAX_TERM_DEPTH + 1, at), b"a"),
            ProtocolErrorEnum::TermTooDeep {
                command: 11,
                section: 0,
                at: 33,
                depth: 33,
                limit: 32
            },
            "depth 33 through part {at}"
        );
    }
    // The golden Operation's value is nested exactly 32 deep.
    let bytes = golden("operation_deep").bytes();
    let view = ultragraphdb_protocol_wire::OperationView::from_frame(&frame_of(&bytes)).unwrap();
    assert_eq!(view.terms().len(), 34);
}
