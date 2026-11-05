/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Declared, DeclaredPredicate, RequestTerm, RequestTermView, TermTable,
    Write, WriteChange, WriteView,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::write::write_tests::golden_write;
use crate::utils::golden_bodies::{
    body_data, frame_of, golden, put, section, with_command_header, with_section,
};

fn refused(index: usize, change: impl FnOnce(&mut Vec<u8>)) -> TestResult<ProtocolErrorEnum> {
    let bytes = with_section(&golden("write")?.bytes()?, 6, index, change);
    Ok(WriteView::from_frame(&frame_of(&bytes)?)
        .err()
        .context("a refusal")?
        .0)
}

#[test]
fn test_the_golden_write_reads_its_changes_and_terms() -> TestResult {
    let bytes = golden("write")?.bytes()?;
    let view = WriteView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(view.len(), 14);
    assert!(!view.is_empty());
    assert_eq!(view.terms().len(), 19);
    assert_eq!(view.terms().get(4), Some(RequestTermView::BlankNode("b1")));
    assert_eq!(
        view.terms().get(15),
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
            predicate: 14,
            object: 15
        })
    );
    assert_eq!(view.change(14), None);
    assert_eq!(view.to_body()?, golden_write()?);
    Ok(())
}

/// The refusal of a Write's field `field` holding `value`.
fn field_value(field: &'static str, value: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command: 8,
        field,
        value,
    }
}

#[test]
fn test_records_are_whole_known_and_zero_where_unused() -> TestResult {
    assert_eq!(
        refused(0, |r| r.truncate(r.len() - 1))?,
        ProtocolErrorEnum::SectionLength {
            command: 8,
            section: 0,
            len: 223,
            unit: 16
        }
    );
    assert_eq!(refused(0, |r| r[0] = 11)?, field_value("change kind", 11));
    assert_eq!(refused(0, |r| r[0] = 0)?, field_value("change kind", 0));
    // Insert takes no flags; DeleteProperty only bit 0; a predicate declaration's kind bits only 0 to 2.
    assert_eq!(refused(0, |r| r[1] = 1)?, field_value("flags", 1));
    assert_eq!(refused(0, |r| r[16 * 6 + 1] = 3)?, field_value("flags", 3));
    assert_eq!(
        refused(0, |r| r[16 * 10 + 1] = 3 | 4)?,
        field_value("flags", 7)
    );
    assert_eq!(
        refused(0, |r| r[16 * 10 + 1] = 64 | 1)?,
        field_value("flags", 65)
    );
    assert_eq!(refused(0, |r| r[2] = 1)?, field_value("change record", 1));
    assert_eq!(
        refused(0, |r| r[16 * 12 + 12] = 1)?,
        field_value("extra", 1)
    );
    Ok(())
}

#[test]
fn test_args_follow_each_other_and_are_consumed_exactly() -> TestResult {
    assert_eq!(
        refused(0, |r| put(r, 16 + 4, 5, 4))?,
        ProtocolErrorEnum::FieldValue {
            command: 8,
            field: "args_start",
            value: 5
        }
    );
    assert_eq!(
        refused(0, |r| put(r, 8, 3, 4))?,
        ProtocolErrorEnum::CountMismatch {
            command: 8,
            field: "change args",
            count: 3,
            expected: 4
        }
    );
    // 63 args: 4, 4, 3, 2, 6, 4, 6, 4, 4, 4 for the changes, 7, 10, 2, 3 for the declarations.
    assert_eq!(
        refused(1, |a| a.extend_from_slice(&0u32.to_le_bytes()))?,
        ProtocolErrorEnum::CountMismatch {
            command: 8,
            field: "args",
            count: 64,
            expected: 63
        }
    );
    // The first declaration's tokenizer count, arg 41 + 2, past its args.
    assert_eq!(
        refused(1, |a| put(a, 4 * 43, 40, 4))?,
        ProtocolErrorEnum::CountMismatch {
            command: 8,
            field: "change args",
            count: 7,
            expected: 43
        }
    );
    Ok(())
}

#[test]
fn test_term_and_name_references_are_in_range() -> TestResult {
    // The Insert's subject, arg 1, past the 19 terms; absent where it may not be.
    assert_eq!(
        refused(1, |a| put(a, 4, 19, 4))?,
        ProtocolErrorEnum::IndexOutOfRange {
            command: 8,
            section: 1,
            at: 1,
            index: 19,
            limit: 19
        }
    );
    assert_eq!(
        refused(1, |a| put(a, 4, u32::MAX.into(), 4))?,
        ProtocolErrorEnum::IndexOutOfRange {
            command: 8,
            section: 1,
            at: 1,
            index: u32::MAX,
            limit: 19
        }
    );
    // The first declaration's predicate, arg 41, past the 15 names.
    assert_eq!(
        refused(1, |a| put(a, 4 * 41, 15, 4))?,
        ProtocolErrorEnum::IndexOutOfRange {
            command: 8,
            section: 1,
            at: 41,
            index: 15,
            limit: 15
        }
    );
    Ok(())
}

#[test]
fn test_a_term_reference_is_bounded_by_the_terms_not_the_names() -> TestResult {
    // Three terms and five names: an Insert's object, arg 3, naming term 3 is out of range though name 3 exists.
    let mut terms = TermTable::new();
    let [a, knows, b] =
        ["a", "knows", "b"].map(|n| terms.intern(RequestTerm::Iri(format!("http://ex.org/{n}"))));
    let [a, knows, b] = [a?, knows?, b?];
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
    let bytes = Write::new(terms, changes).frame(golden("write")?.header())?;
    let view = WriteView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!((view.terms().len(), view.names().len()), (3, 5));
    let bytes = with_section(&bytes, 6, 1, |args| put(args, 4 * 3, 3, 4));
    assert_eq!(
        WriteView::from_frame(&frame_of(&bytes)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::IndexOutOfRange {
            command: 8,
            section: 1,
            at: 3,
            index: 3,
            limit: 3
        }
    );
    Ok(())
}

#[test]
fn test_a_reserved_term_byte_is_named_at_its_body_offset() -> TestResult {
    // The term records are section 2; the first record's reserved bytes start 2 bytes in.
    let records_at = section(body_data(&golden("write")?.bytes()?), 0, 2).0;
    assert_eq!(
        refused(2, |r| r[2] = 1)?,
        ProtocolErrorEnum::PaddingNotZero {
            command: 8,
            offset: records_at + 2
        }
    );
    Ok(())
}

#[test]
fn test_every_command_header_byte_is_unused() -> TestResult {
    // The transaction's Begin names the tenant; a Write's command header is all zero.
    let bytes = golden("write")?.bytes()?;
    for at in [0usize, 7, 8, 127] {
        let frame = with_command_header(&bytes, |h| h[at] = 2);
        assert_eq!(
            WriteView::from_frame(&frame_of(&frame)?)
                .err()
                .context("a refusal")?
                .0,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    Ok(())
}

#[test]
fn test_each_change_kind_takes_its_arg_count() -> TestResult {
    // PROTOCOL.md section 4.6 step 8, record by record of the golden Write (the changes and declarations of
    // `golden_write`): record i's `args_len` (record offset 8) set to `len`, refused at record i before the next
    // record's `args_start` is read: (record, len, the count the kind takes).
    let cases: [(usize, u32, u64); 17] = [
        // Insert and Delete take exactly 4.
        (0, 3, 4),
        (0, 5, 4),
        (1, 3, 4),
        // DeletePattern takes 3, DeleteStatement 2: the graph and the reifier.
        (2, 2, 3),
        (3, 1, 2),
        (3, 3, 2),
        // InsertEdge takes 4 and an even number more: 5 args miss the pair's value, 3 args the object.
        (4, 5, 6),
        (4, 3, 4),
        // SetProperty by its reifier (flag bit 0 clear) takes 2 + 2; DeleteProperty by its quad (bit 0 set) 4 + 2.
        (5, 3, 4),
        (5, 5, 4),
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
            refused(0, |r| put(r, 16 * record + 8, len.into(), 4))?,
            ProtocolErrorEnum::CountMismatch {
                command: 8,
                field: "change args",
                count: len.into(),
                expected
            },
            "record {record} with {len} args"
        );
    }
    // A relation of two names is whole: the one arg it leaves is the args section's, 63 held and 62 consumed.
    assert_eq!(
        refused(0, |r| put(r, 16 * 13 + 8, 2, 4))?,
        ProtocolErrorEnum::CountMismatch {
            command: 8,
            field: "args",
            count: 63,
            expected: 62
        }
    );
    Ok(())
}

#[test]
fn test_each_change_kind_checks_its_term_and_name_args() -> TestResult {
    // PROTOCOL.md section 4.6 step 9 over the golden Write's 63 args (19 terms, 15 names). Record i's args start at
    // the sum of the counts before it, 4, 4, 3, 2, 6, 4, 6, 4, 4, 4, 7, 10, 2, 3: record 5 at 19, 6 at 23, 10 at 41,
    // 13 at 60. (arg, value, limit) for one arg set out of range, refused naming the arg's index in the args section.
    let cases: [(usize, u32, u32); 5] = [
        // DeleteProperty by its quad: the statement's subject is a term.
        (24, 19, 19),
        // SetProperty: its value is a term, never ABSENT (DeleteProperty's may be).
        (22, u32::MAX, 19),
        // A predicate's datatype, which may be ABSENT, is otherwise a name; so is each tokenizer.
        (42, 15, 15),
        (44, 15, 15),
        // A predicate's domain, its last arg.
        (47, 15, 15),
    ];
    for (at, value, limit) in cases {
        assert_eq!(
            refused(1, |a| put(a, 4 * at, value.into(), 4))?,
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
        refused(0, |r| put(r, 16 * 10 + 8, 8, 4))?,
        ProtocolErrorEnum::CountMismatch {
            command: 8,
            field: "change args",
            count: 8,
            expected: 7
        }
    );
    // The last record, the relation of 3 args at 60, claiming 4: 64 args asked of the 63 held.
    assert_eq!(
        refused(0, |r| put(r, 16 * 13 + 8, 4, 4))?,
        ProtocolErrorEnum::CountMismatch {
            command: 8,
            field: "args",
            count: 63,
            expected: 64
        }
    );
    // DeleteStatement (record 3, args 11 and 12) names its statement by its reifier, a term: term 0 read back.
    let bytes = with_section(&golden("write")?.bytes()?, 6, 1, |a| put(a, 4 * 12, 0, 4));
    assert_eq!(
        WriteView::from_frame(&frame_of(&bytes)?)?.change(3),
        Some(WriteChange::DeleteStatement {
            graph: None,
            reifier: 0
        })
    );
    Ok(())
}

#[test]
fn test_each_predicate_flag_reads_back_alone() -> TestResult {
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
        let bytes = write.frame(golden("write")?.header())?;
        assert_eq!(WriteView::from_frame(&frame_of(&bytes)?)?.to_body()?, write);
    }
    Ok(())
}
