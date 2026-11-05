/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    ABSENT, BodyBuilder, BodyView, Command, Declared, DeclaredEntityType, DeclaredPredicate,
    DeclaredRelation, DirectWrite, DirectWriteView, RequestTermView, StatementTarget, TermTable,
    WriteChange,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::direct_write::direct_write_tests::golden_direct_write;
use crate::utils::golden_bodies::{
    body_data, frame_of, golden, put, reframe, section, view_any, with_command_header, with_section,
};

fn command() -> u16 {
    Command::DirectWrite.number()
}

fn golden_bytes() -> TestResult<Vec<u8>> {
    golden("direct_write")?.bytes()
}

/// The length of section `index` of the golden frame.
fn golden_len(index: usize) -> TestResult<usize> {
    Ok(section(body_data(&golden_bytes()?), 0, index).1)
}

fn refusal(bytes: &[u8]) -> TestResult<ProtocolErrorEnum> {
    Ok(DirectWriteView::from_frame(&frame_of(bytes)?)
        .err()
        .context("a refusal")?
        .0)
}

fn header_refused(change: impl FnOnce(&mut [u8])) -> TestResult<ProtocolErrorEnum> {
    refusal(&with_command_header(&golden_bytes()?, change))
}

fn refused(index: usize, change: impl FnOnce(&mut Vec<u8>)) -> TestResult<ProtocolErrorEnum> {
    refusal(&with_section(&golden_bytes()?, 6, index, change))
}

fn field_value(field: &'static str, value: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command: command(),
        field,
        value,
    }
}

fn section_length(index: usize, len: usize, unit: usize) -> ProtocolErrorEnum {
    ProtocolErrorEnum::SectionLength {
        command: command(),
        section: index,
        len,
        unit,
    }
}

#[test]
fn test_the_golden_direct_write_reads_its_values() -> TestResult {
    let bytes = golden_bytes()?;
    let view = DirectWriteView::from_frame(&frame_of(&bytes)?)?;
    let fixture = golden_direct_write()?;
    assert_eq!(view.session(), fixture.session());
    assert_eq!(view.request(), fixture.request());
    assert_eq!(view.namespace(), fixture.namespace());
    assert_eq!(view.token(), fixture.token());
    assert_eq!(view.len(), fixture.changes().len());
    assert!(!view.is_empty());
    assert_eq!(view.terms().len(), fixture.terms().len());
    assert_eq!(view.terms().get(7), Some(RequestTermView::BlankNode("b1")));
    for (index, change) in fixture.changes().iter().enumerate() {
        assert_eq!(view.change(index).as_ref(), Some(change));
    }
    assert_eq!(view.change(fixture.changes().len()), None);
    assert_eq!(view.to_body()?, fixture);
    Ok(())
}

#[test]
fn test_the_view_refuses_another_command() -> TestResult {
    let bytes = golden("write")?.bytes()?;
    assert_eq!(
        refusal(&bytes)?,
        ProtocolErrorEnum::WrongCommand {
            expected: command(),
            found: Command::Write.number()
        }
    );
    Ok(())
}

#[test]
fn test_unused_bytes_56_to_127_are_zero() -> TestResult {
    for at in [56usize, 127] {
        assert_eq!(
            header_refused(|h| h[at] = 1)?,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    Ok(())
}

#[test]
fn test_a_request_with_no_session_is_refused() -> TestResult {
    let request = golden_direct_write()?.request();
    assert_eq!(
        header_refused(|h| h[..16].fill(0))?,
        field_value("request", request)
    );
    Ok(())
}

#[test]
fn test_a_session_with_request_0_is_refused() -> TestResult {
    assert_eq!(
        header_refused(|h| h[16..24].fill(0))?,
        field_value("request", 0)
    );
    Ok(())
}

#[test]
fn test_no_session_with_request_0_is_read() -> TestResult {
    let bytes = with_command_header(&golden_bytes()?, |h| h[..24].fill(0));
    let view = DirectWriteView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!((view.session(), view.request()), ([0; 16], 0));
    Ok(())
}

#[test]
fn test_the_token_is_checked_as_a_session_token() -> TestResult {
    assert_eq!(
        header_refused(|h| h[36] = 1)?,
        ProtocolErrorEnum::ReservedNotZero { offset: 128 + 36 }
    );
    assert_eq!(
        header_refused(|h| put(h, 32, 0, 4))?,
        field_value("token", 0)
    );
    assert_eq!(
        header_refused(|h| put(h, 40, 0, 8))?,
        field_value("token", 0)
    );
    assert_eq!(
        header_refused(|h| put(h, 48, 0, 8))?,
        field_value("token", 0)
    );
    assert_eq!(
        header_refused(|h| put(h, 48, 1 << 42, 8))?,
        field_value("token", 1 << 42)
    );
    let none = with_command_header(&golden_bytes()?, |h| h[32..56].fill(0));
    assert_eq!(
        DirectWriteView::from_frame(&frame_of(&none)?)?.token(),
        None
    );
    Ok(())
}

#[test]
fn test_the_sections_are_checked() -> TestResult {
    let bytes = golden_bytes()?;
    let mut data = body_data(&bytes).to_vec();
    let end = data.len();
    data.push(0);
    assert_eq!(
        refusal(&reframe(&bytes, &data))?,
        ProtocolErrorEnum::TrailingBytes {
            command: command(),
            end,
            len: end + 1
        }
    );
    Ok(())
}

#[test]
fn test_the_term_table_is_checked() -> TestResult {
    assert_eq!(
        refused(2, |r| r.push(0))?,
        section_length(2, golden_len(2)? + 1, 24)
    );
    Ok(())
}

#[test]
fn test_the_names_are_checked() -> TestResult {
    assert_eq!(
        refused(4, |e| e.push(0))?,
        section_length(4, golden_len(4)? + 1, 4)
    );
    Ok(())
}

/// The golden frame holding the name `name`.
fn with_name(frame: &[u8], name: &[u8]) -> Vec<u8> {
    let text = with_section(frame, 6, 5, |t| t.extend_from_slice(name));
    with_section(&text, 6, 4, |e| {
        e.extend_from_slice(&u32::try_from(name.len()).unwrap_or(0).to_le_bytes());
    })
}

#[test]
fn test_the_names_hold_no_name() -> TestResult {
    assert_eq!(
        refusal(&with_name(&golden_bytes()?, b"http://ex.org/name"))?,
        ProtocolErrorEnum::CountMismatch {
            command: command(),
            field: "names",
            count: 1,
            expected: 0
        }
    );
    // An empty name is a name.
    assert!(matches!(
        refusal(&with_name(&golden_bytes()?, b""))?,
        ProtocolErrorEnum::CountMismatch { field: "names", .. }
    ));
    // Text no end reaches is the string list's own refusal.
    assert!(matches!(
        refused(5, |t| t.push(b'a'))?,
        ProtocolErrorEnum::CountMismatch {
            field: "string text",
            ..
        }
    ));
    Ok(())
}

#[test]
fn test_the_args_are_a_whole_number_of_u32s() -> TestResult {
    assert_eq!(
        refused(1, |a| a.truncate(a.len() - 1))?,
        section_length(1, golden_len(1)? - 1, 4)
    );
    Ok(())
}

#[test]
fn test_a_direct_write_holding_no_change_is_refused() -> TestResult {
    assert_eq!(
        refused(0, Vec::clear)?,
        ProtocolErrorEnum::CountMismatch {
            command: command(),
            field: "changes",
            count: 0,
            expected: 1
        }
    );
    Ok(())
}

#[test]
fn test_the_records_are_whole_and_known() -> TestResult {
    assert_eq!(
        refused(0, |r| r.truncate(r.len() - 1))?,
        section_length(0, golden_len(0)? - 1, 16)
    );
    for kind in [0u8, 11] {
        assert_eq!(
            refused(0, |r| r[0] = kind)?,
            field_value("change kind", kind.into())
        );
    }
    Ok(())
}

#[test]
fn test_a_record_of_kinds_8_to_10_is_refused_as_a_declaration() -> TestResult {
    for kind in [8u8, 9, 10] {
        assert_eq!(
            refused(0, |r| r[0] = kind)?,
            field_value("record", kind.into()),
            "{kind}"
        );
    }
    // Before the flags the kind would take.
    assert_eq!(
        refused(0, |r| {
            r[0] = 8;
            r[1] = 3;
        })?,
        field_value("record", 8)
    );
    Ok(())
}

#[test]
fn test_a_builder_given_a_declaration_builds_a_frame_the_view_refuses() -> TestResult {
    let bytes = DirectWrite::new(
        0,
        TermTable::new(),
        vec![WriteChange::Declare(Declared::Predicate(
            DeclaredPredicate::new("http://ex.org/name".to_string()),
        ))],
    )
    .frame(golden("direct_write")?.header())?;
    assert_eq!(refusal(&bytes)?, field_value("record", 8));
    // It writes the record alone: sections 4 and 5, the names, stay empty.
    assert_eq!(section(body_data(&bytes), 0, 4).1, 0);
    assert_eq!(section(body_data(&bytes), 0, 5).1, 0);
    Ok(())
}

/// A declaration of each kind, the record holding its well-formed names.
pub fn declarations() -> [Declared; 3] {
    let iri = "http://ex.org/declared";
    [
        Declared::Predicate(DeclaredPredicate::new(iri.to_string())),
        Declared::EntityType(DeclaredEntityType::new(iri, None, false)),
        Declared::Relation(DeclaredRelation::new(iri, vec![iri.to_string()])),
    ]
}

/// A declaration at any record is refused, its names well formed: the rule reads no record's place.
#[test]
fn test_a_declaration_after_a_change_is_refused() -> TestResult {
    for (declared, kind) in declarations().into_iter().zip([8u8, 9, 10]) {
        let mut terms = TermTable::new();
        let a = terms.iri("http://ex.org/a");
        let insert = WriteChange::Insert {
            graph: None,
            subject: a,
            predicate: a,
            object: a,
        };
        let bytes = DirectWrite::new(0, terms, vec![insert, WriteChange::Declare(declared)])
            .frame(golden("direct_write")?.header())?;
        assert_eq!(
            refusal(&bytes)?,
            field_value("record", kind.into()),
            "{kind}"
        );
    }
    Ok(())
}

/// A change of every kind, 1 to 7, the two property kinds by reifier and by triple, over the terms `a`, `p` and `b`.
fn every_kind_of_change(a: u32, p: u32, b: u32) -> Vec<WriteChange> {
    let reifier = StatementTarget::Reifier {
        graph: None,
        reifier: b,
    };
    let triple = StatementTarget::Triple {
        graph: Some(a),
        subject: a,
        predicate: p,
        object: b,
    };
    vec![
        WriteChange::Insert {
            graph: None,
            subject: a,
            predicate: p,
            object: b,
        },
        WriteChange::Delete {
            graph: None,
            subject: a,
            predicate: p,
            object: b,
        },
        WriteChange::InsertEdge {
            graph: None,
            subject: a,
            predicate: p,
            object: b,
            properties: vec![(p, b)],
        },
        WriteChange::DeleteStatement {
            graph: None,
            reifier: b,
        },
        WriteChange::DeletePattern {
            graph: None,
            subject: a,
            predicate: p,
        },
        WriteChange::SetProperty {
            statement: reifier,
            predicate: p,
            value: b,
        },
        WriteChange::DeleteProperty {
            statement: reifier,
            predicate: p,
            value: None,
        },
        WriteChange::SetProperty {
            statement: triple,
            predicate: p,
            value: b,
        },
        WriteChange::DeleteProperty {
            statement: triple,
            predicate: p,
            value: Some(b),
        },
    ]
}

#[test]
fn test_a_direct_write_takes_every_kind_of_change() -> TestResult {
    let mut terms = TermTable::new();
    let (a, p, b) = (
        terms.iri("http://ex.org/a"),
        terms.iri("http://ex.org/p"),
        terms.iri("http://ex.org/b"),
    );
    let write = DirectWrite::new(0, terms, every_kind_of_change(a, p, b));
    let bytes = write.frame(golden("direct_write")?.header())?;
    assert_eq!(
        DirectWriteView::from_frame(&frame_of(&bytes)?)?.to_body()?,
        write
    );
    // Rebuilt byte for byte.
    assert_eq!(view_any(&bytes)?, Ok(()));
    Ok(())
}

#[test]
fn test_the_records_flags_zero_bytes_and_extra_are_as_their_kind_defines() -> TestResult {
    assert_eq!(refused(0, |r| r[1] = 1)?, field_value("flags", 1));
    assert_eq!(refused(0, |r| r[2] = 1)?, field_value("change record", 1));
    assert_eq!(refused(0, |r| r[12] = 1)?, field_value("extra", 1));
    Ok(())
}

#[test]
fn test_the_records_args_follow_each_other_and_are_consumed_exactly() -> TestResult {
    let args = golden_len(1)? as u64 / 4;
    assert_eq!(
        refused(0, |r| put(r, 16 + 4, 5, 4))?,
        field_value("args_start", 5)
    );
    // The first record's args run past the args.
    assert_eq!(
        refused(0, |r| put(r, 8, args + 1, 4))?,
        ProtocolErrorEnum::CountMismatch {
            command: command(),
            field: "args",
            count: args,
            expected: args + 1
        }
    );
    assert_eq!(
        refused(0, |r| put(r, 8, 3, 4))?,
        ProtocolErrorEnum::CountMismatch {
            command: command(),
            field: "change args",
            count: 3,
            expected: 4
        }
    );
    assert_eq!(
        refused(1, |a| a.extend_from_slice(&0u32.to_le_bytes()))?,
        ProtocolErrorEnum::CountMismatch {
            command: command(),
            field: "args",
            count: args + 1,
            expected: args
        }
    );
    Ok(())
}

#[test]
fn test_a_term_arg_is_below_the_table_and_absent_only_where_allowed() -> TestResult {
    let limit = u32::try_from(golden_direct_write()?.terms().len())?;
    assert_eq!(
        refused(1, |a| put(a, 4, limit.into(), 4))?,
        ProtocolErrorEnum::IndexOutOfRange {
            command: command(),
            section: 1,
            at: 1,
            index: limit,
            limit
        }
    );
    assert_eq!(
        refused(1, |a| put(a, 4, ABSENT.into(), 4))?,
        ProtocolErrorEnum::IndexOutOfRange {
            command: command(),
            section: 1,
            at: 1,
            index: ABSENT,
            limit
        }
    );
    Ok(())
}

/// A frame breaking every rule is refused for the first of them, and with that one mended, for the next: the protocol's
/// order, built here from its last check to its first.
#[test]
fn test_the_checks_run_in_the_protocols_order() -> TestResult {
    let mut frame = with_section(&golden_bytes()?, 6, 0, |r| r[0] = 8);
    assert_eq!(refusal(&frame)?, field_value("record", 8));
    frame = with_section(&frame, 6, 0, Vec::clear);
    assert!(matches!(
        refusal(&frame)?,
        ProtocolErrorEnum::CountMismatch {
            field: "changes",
            ..
        }
    ));
    frame = with_section(&frame, 6, 1, |a| a.push(0));
    assert!(matches!(
        refusal(&frame)?,
        ProtocolErrorEnum::SectionLength { section: 1, .. }
    ));
    frame = with_name(&frame, b"http://ex.org/name");
    assert!(matches!(
        refusal(&frame)?,
        ProtocolErrorEnum::CountMismatch { field: "names", .. }
    ));
    frame = with_section(&frame, 6, 4, |e| e.push(0));
    assert!(matches!(
        refusal(&frame)?,
        ProtocolErrorEnum::SectionLength { section: 4, .. }
    ));
    frame = with_section(&frame, 6, 2, |r| r.push(0));
    assert!(matches!(
        refusal(&frame)?,
        ProtocolErrorEnum::SectionLength { section: 2, .. }
    ));
    let mut data = body_data(&frame).to_vec();
    data.push(0);
    frame = reframe(&frame, &data);
    assert!(matches!(
        refusal(&frame)?,
        ProtocolErrorEnum::TrailingBytes { .. }
    ));
    frame = with_command_header(&frame, |h| h[32..36].fill(0));
    assert_eq!(refusal(&frame)?, field_value("token", 0));
    let request = golden_direct_write()?.request();
    frame = with_command_header(&frame, |h| h[..16].fill(0));
    assert_eq!(refusal(&frame)?, field_value("request", request));
    frame = with_command_header(&frame, |h| h[56] = 1);
    assert_eq!(
        refusal(&frame)?,
        ProtocolErrorEnum::ReservedNotZero { offset: 128 + 56 }
    );
    Ok(())
}
