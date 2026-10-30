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
use crate::utils::golden_bodies::{
    body_data, frame_of, golden, put, section, with_command_header, with_section,
};

fn refused(index: usize, change: impl FnOnce(&mut Vec<u8>)) -> ProtocolErrorEnum {
    let bytes = with_section(&golden("write").bytes(), 6, index, change);
    WriteView::from_frame(&frame_of(&bytes)).unwrap_err().0
}

#[test]
fn test_the_golden_write_reads_its_changes_and_terms() {
    let bytes = golden("write").bytes();
    let view = WriteView::from_frame(&frame_of(&bytes)).unwrap();
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
    let bytes = Write::new(terms, changes)
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

#[test]
fn test_every_command_header_byte_is_unused() {
    // The transaction's Begin names the namespace; a Write's command header is all zero.
    let bytes = golden("write").bytes();
    for at in [0usize, 7, 8, 127] {
        let frame = with_command_header(&bytes, |h| h[at] = 2);
        assert_eq!(
            WriteView::from_frame(&frame_of(&frame)).unwrap_err().0,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
}

#[test]
fn test_each_change_kind_takes_its_arg_count() {
    // PROTOCOL.md section 4.6 step 8, record by record of the golden Write (the changes and declarations of
    // `golden_write`): record i's `args_len` (record offset 8) set to `len`, refused at record i before the next
    // record's `args_start` is read: (record, len, the count the kind takes).
    let cases: [(usize, u32, u64); 17] = [
        // Insert and Delete take exactly 4.
        (0, 3, 4),
        (0, 5, 4),
        (1, 3, 4),
        // DeletePattern and DeleteStatement take 3.
        (2, 2, 3),
        (3, 2, 3),
        (3, 4, 3),
        // InsertEdge takes 4 and an even number more: 5 args miss the pair's value, 3 args the object.
        (4, 5, 6),
        (4, 3, 4),
        // SetProperty by its ID (flag bit 0 clear) takes 3 + 2; DeleteProperty by its triple (bit 0 set) 4 + 2.
        (5, 4, 5),
        (5, 6, 5),
        (6, 5, 6),
        (6, 7, 6),
        // The first predicate, [name, datatype, 2, term, exact, 1, Person]: six args cut its one domain.
        (10, 6, 7),
        // The second, [embedding, ABSENT, 0, 0, hnsw, 2, metric, euclidean, maxLevels, 3]: nine cut an option.
        (11, 9, 10),
        // An entity type takes 2, a relation at least 1.
        (12, 1, 2),
        (12, 3, 2),
        (13, 0, 1),
    ];
    for (record, len, expected) in cases {
        assert_eq!(
            refused(0, |r| put(r, 16 * record + 8, len.into(), 4)),
            ProtocolErrorEnum::CountMismatch {
                command: 8,
                field: "change args",
                count: len.into(),
                expected
            },
            "record {record} with {len} args"
        );
    }
    // A relation of two names is whole: the one arg it leaves is the args section's, 65 held and 64 consumed.
    assert_eq!(
        refused(0, |r| put(r, 16 * 13 + 8, 2, 4)),
        ProtocolErrorEnum::CountMismatch {
            command: 8,
            field: "args",
            count: 65,
            expected: 64
        }
    );
}

#[test]
fn test_each_change_kind_checks_its_term_and_name_args() {
    // PROTOCOL.md section 4.6 step 9 over the golden Write's 65 args (17 terms, 15 names). Record i's args start at
    // the sum of the counts before it, 4, 4, 3, 3, 6, 5, 6, 4, 4, 4, 7, 10, 2, 3: record 5 at 20, 6 at 25, 10 at 43,
    // 13 at 62. (arg, value, limit) for one arg set out of range, refused naming the arg's index in the args section.
    let cases: [(usize, u32, u32); 5] = [
        // DeleteProperty by its triple: the statement's subject is a term.
        (26, 17, 17),
        // SetProperty: its value is a term, never ABSENT (DeleteProperty's may be).
        (24, u32::MAX, 17),
        // A predicate's datatype, which may be ABSENT, is otherwise a name; so is each tokenizer.
        (44, 15, 15),
        (46, 15, 15),
        // A predicate's domain, its last arg.
        (49, 15, 15),
    ];
    for (at, value, limit) in cases {
        assert_eq!(
            refused(1, |a| put(a, 4 * at, value.into(), 4)),
            ProtocolErrorEnum::IndexOutOfRange {
                command: 8,
                section: 1,
                at,
                index: value,
                limit
            },
            "arg {at}"
        );
    }
    // A predicate's args are consumed exactly: the first predicate's seven args and one more.
    assert_eq!(
        refused(0, |r| put(r, 16 * 10 + 8, 8, 4)),
        ProtocolErrorEnum::CountMismatch {
            command: 8,
            field: "change args",
            count: 8,
            expected: 7
        }
    );
    // The last record, the relation of 3 args at 62, claiming 4: 66 args asked of the 65 held.
    assert_eq!(
        refused(0, |r| put(r, 16 * 13 + 8, 4, 4)),
        ProtocolErrorEnum::CountMismatch {
            command: 8,
            field: "args",
            count: 65,
            expected: 66
        }
    );
    // The ID halves of DeleteStatement (record 3, args 11 to 13) take any value: low 0xFFFF_FFFF, high 0x8000_0000
    // are the ID 0x8000_0000_FFFF_FFFF.
    let bytes = with_section(&golden("write").bytes(), 6, 1, |a| {
        put(a, 4 * 12, 0xFFFF_FFFF, 4);
        put(a, 4 * 13, 0x8000_0000, 4);
    });
    assert_eq!(
        WriteView::from_frame(&frame_of(&bytes)).unwrap().change(3),
        Some(WriteChange::DeleteStatement {
            graph: None,
            id: 0x8000_0000_FFFF_FFFF
        })
    );
}

#[test]
fn test_each_predicate_flag_reads_back_alone() {
    // Flag bits 2 to 4 of a predicate declaration (single, unique, count index), each set alone, read back as that
    // property alone: a decoder reading one bit for another fails here, where the golden predicate sets all three.
    let base = || DeclaredPredicate::new("http://ex.org/p".to_string());
    for declared in [
        base().with_single(true),
        base().with_unique(true),
        base().with_count_index(true),
    ] {
        let write = Write::new(
            TermTable::new(),
            vec![WriteChange::Declare(Declared::Predicate(declared))],
        );
        let bytes = write.frame(golden("write").header()).unwrap();
        assert_eq!(
            WriteView::from_frame(&frame_of(&bytes))
                .unwrap()
                .to_body()
                .unwrap(),
            write
        );
    }
}
