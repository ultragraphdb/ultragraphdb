/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Declared, DeclaredPredicate, RequestTerm, RequestTermView, TermTable,
    Write, WriteChange, WriteView,
};

use crate::types::write::write_tests::golden_write;
use crate::utils::golden_bodies::{body_data, frame_of, golden, put, section, with_section};

fn refused(index: usize, change: impl FnOnce(&mut Vec<u8>)) -> ProtocolErrorEnum {
    let bytes = with_section(&golden("write").bytes(), 6, index, change);
    WriteView::from_frame(&frame_of(&bytes)).unwrap_err().0
}

#[test]
fn test_the_golden_write_reads_its_changes_and_terms() {
    let bytes = golden("write").bytes();
    let view = WriteView::from_frame(&frame_of(&bytes)).unwrap();
    assert_eq!(view.namespace(), 2);
    assert_eq!(view.len(), 14);
    assert!(!view.is_empty());
    assert_eq!(view.terms().len(), 17);
    assert_eq!(view.terms().get(4), Some(RequestTermView::BlankNode("b1")));
    assert_eq!(
        view.terms().get(13),
        Some(RequestTermView::Triple {
            subject: 0,
            predicate: 1,
            object: 2
        })
    );
    assert_eq!(view.names().get(0), Some("http://ex.org/name"));
    assert_eq!(
        view.change(7),
        Some(WriteChange::Insert {
            graph: None,
            subject: 0,
            predicate: 12,
            object: 13
        })
    );
    assert_eq!(view.change(14), None);
    assert_eq!(view.to_body().unwrap(), golden_write());
}

#[test]
fn test_records_are_whole_known_and_zero_where_unused() {
    assert_eq!(
        refused(0, |r| r.truncate(r.len() - 1)),
        ProtocolErrorEnum::SectionLength {
            command: 8,
            section: 0,
            len: 223,
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
        refused(0, |r| r[16 * 10 + 1] = 3 | 4),
        ProtocolErrorEnum::FieldValue {
            command: 8,
            field: "flags",
            value: 7
        }
    );
    assert_eq!(
        refused(0, |r| r[16 * 10 + 1] = 64 | 1),
        ProtocolErrorEnum::FieldValue {
            command: 8,
            field: "flags",
            value: 65
        }
    );
    assert_eq!(
        refused(0, |r| r[2] = 1),
        ProtocolErrorEnum::FieldValue {
            command: 8,
            field: "change record",
            value: 1
        }
    );
    assert_eq!(
        refused(0, |r| r[16 * 12 + 12] = 1),
        ProtocolErrorEnum::FieldValue {
            command: 8,
            field: "extra",
            value: 1
        }
    );
}

#[test]
fn test_args_follow_each_other_and_are_consumed_exactly() {
    assert_eq!(
        refused(0, |r| put(r, 16 + 4, 5, 4)),
        ProtocolErrorEnum::FieldValue {
            command: 8,
            field: "args_start",
            value: 5
        }
    );
    assert_eq!(
        refused(0, |r| put(r, 8, 3, 4)),
        ProtocolErrorEnum::CountMismatch {
            command: 8,
            field: "change args",
            count: 3,
            expected: 4
        }
    );
    // 65 args: 4, 4, 3, 3, 6, 5, 6, 4, 4, 4 for the changes, 7, 10, 2, 3 for the declarations.
    assert_eq!(
        refused(1, |a| a.extend_from_slice(&0u32.to_le_bytes())),
        ProtocolErrorEnum::CountMismatch {
            command: 8,
            field: "args",
            count: 66,
            expected: 65
        }
    );
    // The first declaration's tokenizer count, arg 43 + 2, past its args.
    assert_eq!(
        refused(1, |a| put(a, 4 * 45, 40, 4)),
        ProtocolErrorEnum::CountMismatch {
            command: 8,
            field: "change args",
            count: 7,
            expected: 43
        }
    );
}

#[test]
fn test_term_and_name_references_are_in_range() {
    // The Insert's subject, arg 1, past the 17 terms; absent where it may not be.
    assert_eq!(
        refused(1, |a| put(a, 4, 17, 4)),
        ProtocolErrorEnum::IndexOutOfRange {
            command: 8,
            section: 1,
            at: 1,
            index: 17,
            limit: 17
        }
    );
    assert_eq!(
        refused(1, |a| put(a, 4, u32::MAX.into(), 4)),
        ProtocolErrorEnum::IndexOutOfRange {
            command: 8,
            section: 1,
            at: 1,
            index: u32::MAX,
            limit: 17
        }
    );
    // The first declaration's predicate, arg 43, past the 15 names.
    assert_eq!(
        refused(1, |a| put(a, 4 * 43, 15, 4)),
        ProtocolErrorEnum::IndexOutOfRange {
            command: 8,
            section: 1,
            at: 43,
            index: 15,
            limit: 15
        }
    );
}

#[test]
fn test_a_term_reference_is_bounded_by_the_terms_not_the_names() {
    // Three terms and five names: an Insert's object, arg 3, naming term 3 is out of range though name 3 exists.
    let mut terms = TermTable::new();
    let [a, knows, b] = ["a", "knows", "b"].map(|n| {
        terms
            .intern(RequestTerm::Iri(format!("http://ex.org/{n}")))
            .unwrap()
    });
    let declared = DeclaredPredicate::new("http://ex.org/name".to_string())
        .with_datatype("http://www.w3.org/2001/XMLSchema#string".to_string())
        .with_tokenizers(vec!["term".to_string(), "exact".to_string()])
        .with_domains(vec!["http://ex.org/Person".to_string()]);
    let changes = vec![
        WriteChange::Insert {
            graph: None,
            subject: a,
            predicate: knows,
            object: b,
        },
        WriteChange::Declare(Declared::Predicate(declared)),
    ];
    let bytes = Write::new(2, terms, changes)
        .frame(golden("write").header())
        .unwrap();
    let view = WriteView::from_frame(&frame_of(&bytes)).unwrap();
    assert_eq!((view.terms().len(), view.names().len()), (3, 5));
    let bytes = with_section(&bytes, 6, 1, |a| put(a, 4 * 3, 3, 4));
    assert_eq!(
        WriteView::from_frame(&frame_of(&bytes)).unwrap_err().0,
        ProtocolErrorEnum::IndexOutOfRange {
            command: 8,
            section: 1,
            at: 3,
            index: 3,
            limit: 3
        }
    );
}

#[test]
fn test_a_reserved_term_byte_is_named_at_its_body_offset() {
    // The term records are section 2; the first record's reserved bytes start 2 bytes in.
    let records_at = section(body_data(&golden("write").bytes()), 0, 2).0;
    assert_eq!(
        refused(2, |r| r[2] = 1),
        ProtocolErrorEnum::PaddingNotZero {
            command: 8,
            offset: records_at + 2
        }
    );
}
