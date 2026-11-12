/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    ABSENT, BodyBuilder, BodyView, Command, CommitOperation, ConditionalWrite,
    ConditionalWriteView, Link, StatementTarget, TermTable, WriteChange,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::conditional_write::conditional_write_tests::{
    golden_conditional_write, golden_operation_only,
};
use crate::types::direct_write_view::direct_write_view_tests::declarations;
use crate::utils::golden_bodies::{
    body_data, frame_of, golden, put, reframe, section, view_any, with_command_header, with_section,
};

fn command() -> u16 {
    Command::ConditionalWrite.number()
}

fn golden_bytes() -> TestResult<Vec<u8>> {
    golden("conditional_write")?.bytes()
}

/// The length of section `index` of the golden frame.
fn golden_len(index: usize) -> TestResult<usize> {
    Ok(section(body_data(&golden_bytes()?), 0, index).1)
}

fn refusal(bytes: &[u8]) -> TestResult<ProtocolErrorEnum> {
    Ok(ConditionalWriteView::from_frame(&frame_of(bytes)?)
        .err()
        .context("a refusal")?
        .0)
}

fn header_refused(change: impl FnOnce(&mut [u8])) -> TestResult<ProtocolErrorEnum> {
    refusal(&with_command_header(&golden_bytes()?, change))
}

fn refused(index: usize, change: impl FnOnce(&mut Vec<u8>)) -> TestResult<ProtocolErrorEnum> {
    refusal(&with_section(&golden_bytes()?, 7, index, change))
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

/// The number of args `operation` takes, as `PROTOCOL.md` §7.3's Count says.
fn count(operation: &CommitOperation) -> usize {
    match operation {
        CommitOperation::InsertIfAbsent { properties, .. } => 3 + 2 * properties.len(),
        CommitOperation::UpsertAndLink {
            properties, links, ..
        } => 3 + 2 * properties.len() + 3 * links.len(),
        CommitOperation::CompareAndSet { .. } => 5,
        CommitOperation::CounterAdd { .. } => 3,
        CommitOperation::DeleteInstance { .. } | CommitOperation::RetractInstance { .. } => 2,
    }
}

/// The arg golden operation `index` starts at: after the insert's four and the operations before it.
fn start_of(index: usize) -> TestResult<usize> {
    let fixture = golden_conditional_write()?;
    Ok(4 + fixture.operations()[..index]
        .iter()
        .map(count)
        .sum::<usize>())
}

/// Golden operation `index`, its fields' byte `at`, within section 6.
fn operation_byte(index: usize, at: usize) -> usize {
    24 * index + at
}

#[test]
fn test_the_golden_conditional_write_reads_its_values() -> TestResult {
    let bytes = golden_bytes()?;
    let view = ConditionalWriteView::from_frame(&frame_of(&bytes)?)?;
    let fixture = golden_conditional_write()?;
    assert_eq!(view.session(), fixture.session());
    assert_eq!(view.request(), fixture.request());
    assert_eq!(view.tenant(), fixture.tenant());
    assert_eq!(view.token(), fixture.token());
    assert_eq!(view.terms().len(), fixture.terms().len());
    assert_eq!(view.change_count(), fixture.changes().len());
    assert_eq!(view.change(0).as_ref(), fixture.changes().first());
    assert_eq!(view.change(fixture.changes().len()), None);
    assert_eq!(view.operation_count(), fixture.operations().len());
    for (index, operation) in fixture.operations().iter().enumerate() {
        assert_eq!(view.operation(index).as_ref(), Some(operation));
    }
    assert_eq!(view.operation(fixture.operations().len()), None);
    assert_eq!(view.to_body()?, fixture);
    Ok(())
}

#[test]
fn test_section_0_may_be_empty_with_no_session_and_no_token() -> TestResult {
    let bytes = golden("conditional_write_operation_only")?.bytes()?;
    let view = ConditionalWriteView::from_frame(&frame_of(&bytes)?)?;
    let fixture = golden_operation_only();
    assert_eq!(view.session(), fixture.session());
    assert_eq!(view.token(), None);
    assert_eq!(view.change_count(), 0);
    assert_eq!(view.change(0), None);
    assert_eq!(view.operation(0).as_ref(), fixture.operations().first());
    assert_eq!(view.to_body()?, fixture);
    Ok(())
}

#[test]
fn test_the_view_refuses_another_command() -> TestResult {
    let bytes = golden("direct_write")?.bytes()?;
    assert_eq!(
        refusal(&bytes)?,
        ProtocolErrorEnum::WrongCommand {
            expected: command(),
            found: Command::DirectWrite.number()
        }
    );
    Ok(())
}

#[test]
fn test_unused_bytes_64_to_127_are_zero() -> TestResult {
    for at in [64usize, 127] {
        assert_eq!(
            header_refused(|h| h[at] = 1)?,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    Ok(())
}

#[test]
fn test_a_request_with_no_session_is_refused() -> TestResult {
    let request = golden_conditional_write()?.request();
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
fn test_the_token_is_checked_as_a_session_token() -> TestResult {
    assert_eq!(
        header_refused(|h| h[36] = 1)?,
        ProtocolErrorEnum::ReservedNotZero { offset: 128 + 36 }
    );
    assert_eq!(
        header_refused(|h| put(h, 32, 0, 4))?,
        field_value("token", 0)
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
    let text = with_section(frame, 7, 5, |t| t.extend_from_slice(name));
    with_section(&text, 7, 4, |e| {
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
fn test_section_6_is_a_whole_number_of_operations() -> TestResult {
    assert_eq!(
        refused(6, |o| o.truncate(o.len() - 1))?,
        section_length(6, golden_len(6)? - 1, 24)
    );
    Ok(())
}

#[test]
fn test_a_conditional_write_holding_no_operation_is_refused() -> TestResult {
    assert_eq!(
        refused(6, Vec::clear)?,
        ProtocolErrorEnum::CountMismatch {
            command: command(),
            field: "operations",
            count: 0,
            expected: 1
        }
    );
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
    Ok(())
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
        let counter = CommitOperation::CounterAdd {
            graph: None,
            subject: a,
            predicate: a,
            delta: 1,
        };
        let write = ConditionalWrite::new(
            0,
            terms,
            vec![insert, WriteChange::Declare(declared)],
            vec![counter],
        );
        let bytes = write.frame(golden("conditional_write")?.header())?;
        assert_eq!(
            refusal(&bytes)?,
            field_value("record", kind.into()),
            "{kind}"
        );
    }
    Ok(())
}

/// A property's set and delete, kinds 6 and 7, are changes, and read back beside an operation.
#[test]
fn test_a_conditional_write_takes_the_property_changes() -> TestResult {
    let mut terms = TermTable::new();
    let (a, p, b) = (
        terms.iri("http://ex.org/a"),
        terms.iri("http://ex.org/p"),
        terms.iri("http://ex.org/b"),
    );
    let statement = StatementTarget::Reifier {
        graph: None,
        reifier: b,
    };
    let write = ConditionalWrite::new(
        0,
        terms,
        vec![
            WriteChange::SetProperty {
                statement,
                predicate: p,
                value: b,
            },
            WriteChange::DeleteProperty {
                statement,
                predicate: p,
                value: None,
            },
        ],
        vec![CommitOperation::CounterAdd {
            graph: None,
            subject: a,
            predicate: p,
            delta: 1,
        }],
    );
    let bytes = write.frame(golden("conditional_write")?.header())?;
    assert_eq!(
        ConditionalWriteView::from_frame(&frame_of(&bytes)?)?.to_body()?,
        write
    );
    Ok(())
}

#[test]
fn test_the_records_are_checked_before_the_operations() -> TestResult {
    let bytes = with_section(&golden_bytes()?, 7, 6, |o| o[0] = 0);
    assert_eq!(
        refusal(&with_section(&bytes, 7, 0, |r| r[0] = 11))?,
        field_value("change kind", 11)
    );
    assert_eq!(
        refused(0, |r| put(r, 4, 1, 4))?,
        field_value("args_start", 1)
    );
    assert_eq!(
        refused(0, |r| put(r, 8, u64::from(u32::MAX), 4))?,
        ProtocolErrorEnum::CountMismatch {
            command: command(),
            field: "args",
            count: golden_len(1)? as u64 / 4,
            expected: u64::from(u32::MAX)
        }
    );
    Ok(())
}

#[test]
fn test_an_operations_unused_bytes_are_zero_at_their_body_offset() -> TestResult {
    let (offset, _) = section(body_data(&golden_bytes()?), 0, 6);
    for at in [1usize, 7] {
        assert_eq!(
            refused(6, |o| o[operation_byte(1, at)] = 1)?,
            ProtocolErrorEnum::PaddingNotZero {
                command: command(),
                offset: offset + operation_byte(1, at)
            }
        );
    }
    Ok(())
}

#[test]
fn test_an_operations_kind_is_1_to_6() -> TestResult {
    // PROTOCOL.md §6.7 and §7.3: kinds 1 to 6; 0 and 7 are one below and one above them.
    for kind in [0u8, 7] {
        assert_eq!(
            refused(6, |o| o[0] = kind)?,
            field_value("kind", kind.into())
        );
    }
    Ok(())
}

#[test]
fn test_an_operations_fields_fit_its_kind() -> TestResult {
    assert_eq!(
        refused(6, |o| put(o, operation_byte(0, 8), 1, 8))?,
        field_value("delta", 1)
    );
    assert_eq!(
        refused(6, |o| put(o, operation_byte(2, 16), 1, 4))?,
        field_value("properties", 1)
    );
    assert_eq!(
        refused(6, |o| put(o, operation_byte(0, 20), 1, 4))?,
        field_value("links", 1)
    );
    Ok(())
}

#[test]
fn test_an_operations_link_direction_is_0_or_1() -> TestResult {
    // The upsert-and-link's direction is its last arg.
    let direction = start_of(1)? + count(&golden_conditional_write()?.operations()[1]) - 1;
    assert_eq!(
        refused(1, |a| put(a, 4 * direction, 2, 4))?,
        field_value("link direction", 2)
    );
    Ok(())
}

#[test]
fn test_an_operations_args_are_below_the_table_and_only_its_first_may_be_absent() -> TestResult {
    let limit = u32::try_from(golden_conditional_write()?.terms().len())?;
    let at = start_of(1)?;
    assert_eq!(
        refused(1, |a| put(a, 4 * at, limit.into(), 4))?,
        ProtocolErrorEnum::IndexOutOfRange {
            command: command(),
            section: 1,
            at,
            index: limit,
            limit
        }
    );
    assert_eq!(
        refused(1, |a| put(a, 4 * (at + 1), ABSENT.into(), 4))?,
        ProtocolErrorEnum::IndexOutOfRange {
            command: command(),
            section: 1,
            at: at + 1,
            index: ABSENT,
            limit
        }
    );
    // The upsert-and-link's first arg, its graph, may be absent.
    let absent = with_section(&golden_bytes()?, 7, 1, |a| put(a, 4 * at, ABSENT.into(), 4));
    let view = ConditionalWriteView::from_frame(&frame_of(&absent)?)?;
    assert!(matches!(
        view.operation(1),
        Some(CommitOperation::UpsertAndLink { graph: None, .. })
    ));
    Ok(())
}

#[test]
fn test_an_operation_running_past_the_args_is_refused_at_its_start() -> TestResult {
    let last = golden_conditional_write()?.operations().len() - 1;
    assert_eq!(
        refused(1, |a| a.truncate(a.len() - 4))?,
        field_value("args_start", start_of(last)? as u64)
    );
    // A count no args section holds.
    assert_eq!(
        refused(6, |o| put(o, operation_byte(0, 16), u64::from(u32::MAX), 4))?,
        field_value("args_start", start_of(0)? as u64)
    );
    Ok(())
}

#[test]
fn test_args_left_after_the_last_operation_are_refused() -> TestResult {
    let args = golden_len(1)? as u64 / 4;
    assert_eq!(
        refused(1, |a| a.extend_from_slice(&0u32.to_le_bytes()))?,
        field_value("args_start", args)
    );
    Ok(())
}

/// Inside one operation, `PROTOCOL.md` §7.3's order: its unused bytes, its `kind`, `delta`, `properties` and `links`,
/// the count of its args, and each arg.
#[test]
fn test_an_operation_is_checked_unused_bytes_then_fields_then_args() -> TestResult {
    let limit = u32::try_from(golden_conditional_write()?.terms().len())?;
    // Operation 2 takes no delta, properties or links, and its arg 1 lies past the table.
    let (index, at) = (2, start_of(2)? + 1);
    let mut frame = with_section(&golden_bytes()?, 7, 1, |a| put(a, 4 * at, limit.into(), 4));
    assert_eq!(
        refusal(&frame)?,
        ProtocolErrorEnum::IndexOutOfRange {
            command: command(),
            section: 1,
            at,
            index: limit,
            limit
        }
    );
    // Operation 0's arg 1 lies past the table, and its count past the args: the count comes first.
    let first_arg = start_of(0)? + 1;
    let past_the_args = with_section(
        &with_section(&golden_bytes()?, 7, 1, |a| {
            put(a, 4 * first_arg, limit.into(), 4);
        }),
        7,
        6,
        |o| put(o, operation_byte(0, 16), u64::from(u32::MAX), 4),
    );
    assert_eq!(
        refusal(&past_the_args)?,
        field_value("args_start", start_of(0)? as u64)
    );
    frame = with_section(&frame, 7, 6, |o| put(o, operation_byte(index, 20), 1, 4));
    assert_eq!(refusal(&frame)?, field_value("links", 1));
    frame = with_section(&frame, 7, 6, |o| put(o, operation_byte(index, 16), 1, 4));
    assert_eq!(refusal(&frame)?, field_value("properties", 1));
    frame = with_section(&frame, 7, 6, |o| put(o, operation_byte(index, 8), 1, 8));
    assert_eq!(refusal(&frame)?, field_value("delta", 1));
    frame = with_section(&frame, 7, 6, |o| o[operation_byte(index, 0)] = 0);
    assert_eq!(refusal(&frame)?, field_value("kind", 0));
    frame = with_section(&frame, 7, 6, |o| o[operation_byte(index, 1)] = 1);
    let (offset, _) = section(body_data(&frame), 0, 6);
    assert_eq!(
        refusal(&frame)?,
        ProtocolErrorEnum::PaddingNotZero {
            command: command(),
            offset: offset + operation_byte(index, 1)
        }
    );
    Ok(())
}

/// The operations are checked in order: an earlier one's args before a later one's fields.
#[test]
fn test_an_earlier_operations_args_come_before_a_later_ones_fields() -> TestResult {
    let limit = u32::try_from(golden_conditional_write()?.terms().len())?;
    let at = start_of(0)? + 1;
    let bytes = with_section(
        &with_section(&golden_bytes()?, 7, 1, |a| put(a, 4 * at, limit.into(), 4)),
        7,
        6,
        |o| o[operation_byte(1, 0)] = 0,
    );
    assert_eq!(
        refusal(&bytes)?,
        ProtocolErrorEnum::IndexOutOfRange {
            command: command(),
            section: 1,
            at,
            index: limit,
            limit
        }
    );
    Ok(())
}

/// A frame breaking every rule is refused for the first of them, and with that one mended, for the next: the protocol's
/// order, built here from its last check to its first.
#[test]
fn test_the_checks_run_in_the_protocols_order() -> TestResult {
    let mut frame = with_section(&golden_bytes()?, 7, 1, |a| a.extend_from_slice(&[0; 4]));
    assert!(matches!(
        refusal(&frame)?,
        ProtocolErrorEnum::FieldValue {
            field: "args_start",
            ..
        }
    ));
    frame = with_section(&frame, 7, 6, |o| o[0] = 0);
    assert_eq!(refusal(&frame)?, field_value("kind", 0));
    frame = with_section(&frame, 7, 0, |r| r[0] = 8);
    assert_eq!(refusal(&frame)?, field_value("record", 8));
    frame = with_section(&frame, 7, 6, Vec::clear);
    assert!(matches!(
        refusal(&frame)?,
        ProtocolErrorEnum::CountMismatch {
            field: "operations",
            ..
        }
    ));
    frame = with_section(&frame, 7, 6, |o| o.push(0));
    assert!(matches!(
        refusal(&frame)?,
        ProtocolErrorEnum::SectionLength { section: 6, .. }
    ));
    frame = with_section(&frame, 7, 1, |a| a.push(0));
    assert!(matches!(
        refusal(&frame)?,
        ProtocolErrorEnum::SectionLength { section: 1, .. }
    ));
    frame = with_name(&frame, b"http://ex.org/name");
    assert!(matches!(
        refusal(&frame)?,
        ProtocolErrorEnum::CountMismatch { field: "names", .. }
    ));
    frame = with_section(&frame, 7, 4, |e| e.push(0));
    assert!(matches!(
        refusal(&frame)?,
        ProtocolErrorEnum::SectionLength { section: 4, .. }
    ));
    frame = with_section(&frame, 7, 2, |r| r.push(0));
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
    let request = golden_conditional_write()?.request();
    frame = with_command_header(&frame, |h| h[..16].fill(0));
    assert_eq!(refusal(&frame)?, field_value("request", request));
    Ok(())
}

/// An upsert-and-link's links, one each way, read back after a change's args, and the frame rebuilds byte for byte.
#[test]
fn test_a_built_write_reads_back_links_of_both_directions() -> TestResult {
    let mut terms = TermTable::new();
    let (a, p, b) = (
        terms.iri("http://ex.org/a"),
        terms.iri("http://ex.org/p"),
        terms.iri("http://ex.org/b"),
    );
    let write = ConditionalWrite::new(
        0,
        terms,
        vec![WriteChange::DeletePattern {
            graph: None,
            subject: a,
            predicate: p,
        }],
        vec![CommitOperation::UpsertAndLink {
            graph: None,
            predicate: p,
            value: b,
            properties: Vec::new(),
            links: vec![Link::from_node(p, a), Link::to_node(p, b)],
        }],
    );
    let bytes = write.frame(golden("conditional_write")?.header())?;
    assert_eq!(
        ConditionalWriteView::from_frame(&frame_of(&bytes)?)?.to_body()?,
        write
    );
    assert_eq!(view_any(&bytes)?, Ok(()));
    Ok(())
}

/// The relayed frame differs from the client's by the principal alone; only the client endpoint holds it to 0, after the
/// token and before the sections (`PROTOCOL.md` §6.6).
#[test]
fn test_a_principal_is_read_and_refused_from_a_client() -> TestResult {
    let sent = golden_bytes()?;
    let sent_view = ConditionalWriteView::from_client_frame(&frame_of(&sent)?)?;
    assert_eq!(sent_view.principal(), 0);
    let relayed = with_command_header(&sent, |h| put(h, 56, 9, 8));
    let view = ConditionalWriteView::from_frame(&frame_of(&relayed)?)?;
    assert_eq!(view.principal(), 9);
    assert_eq!(
        ConditionalWriteView::from_client_frame(&frame_of(&relayed)?)
            .err()
            .context("a refusal")?
            .0,
        field_value("principal", 9)
    );
    let short_ends = with_section(&relayed, 7, 4, |ends| ends.extend_from_slice(&[0; 3]));
    assert!(
        ConditionalWriteView::from_frame(&frame_of(&short_ends)?).is_err(),
        "section 4 breaks a rule"
    );
    assert_eq!(
        ConditionalWriteView::from_client_frame(&frame_of(&short_ends)?)
            .err()
            .context("a refusal")?
            .0,
        field_value("principal", 9)
    );
    assert_eq!(view.to_body()?.principal(), 9);
    assert_eq!(view_any(&relayed)?, Ok(()));
    Ok(())
}

/// Stage 6.6: a ConditionalWrite's operations take kinds 5 and 6 (`PROTOCOL.md` §6.7, §7.3), over a ConditionalWrite
/// laid out by hand: tenant 4 at 24, through no session; no change; the args, two IRIs, no name, and two operations.
mod stage_6 {
    use ultragraphdb_error_protocol::ProtocolErrorEnum;
    use ultragraphdb_protocol_wire::{
        ABSENT, BodyBuilder, BodyView, Command, CommitOperation, ConditionalWrite,
        ConditionalWriteView, Header, TermTable,
    };
    use ultragraphdb_tool_bdd::TestResult;

    use crate::utils::golden_bodies::{CLUSTER, frame_of};
    use crate::utils::ugs_frames::{frame, iri_terms, refused, u32s};

    const A: &str = "https://example.org/deep_brain#Node/ORG%3Aacme";
    const B: &str = "https://example.org/deep_brain#Node/ORG%3Abeta";

    /// An operation's 24 bytes: `kind`, 7 zero bytes, `delta`, `properties` and `links` 0.
    fn fields(kind: u8) -> Vec<u8> {
        let mut out = vec![0u8; 24];
        if let Some(byte) = out.first_mut() {
            *byte = kind;
        }
        out
    }

    fn conditional(kinds: [u8; 2]) -> TestResult<Vec<u8>> {
        let [records, text] = iri_terms(&[A, B]);
        frame(
            Command::ConditionalWrite,
            &[(24, &4u64.to_le_bytes())],
            &[
                vec![],
                u32s(&[ABSENT, 0, ABSENT, 1]),
                records,
                text,
                vec![],
                vec![],
                [fields(kinds[0]), fields(kinds[1])].concat(),
            ],
            false,
        )
    }

    /// A delete of `A` and a retraction of `B`, built and read back.
    #[test]
    fn test_a_conditional_write_holds_a_delete_and_a_retraction() -> TestResult {
        let mut terms = TermTable::new();
        let (a, b) = (terms.iri(A), terms.iri(B));
        let operations = vec![
            CommitOperation::DeleteInstance {
                graph: None,
                instance: a,
            },
            CommitOperation::RetractInstance {
                graph: None,
                instance: b,
            },
        ];
        let write = ConditionalWrite::new(4, terms, vec![], operations.clone());
        let bytes = write.frame(Header::new(CLUSTER, Command::ConditionalWrite).with_stream(3))?;
        assert_eq!(bytes, conditional([5, 6])?);
        let view = ConditionalWriteView::from_frame(&frame_of(&bytes)?)?;
        assert_eq!(view.operation_count(), 2);
        assert_eq!(view.operation(0), operations.first().cloned());
        assert_eq!(view.operation(1), operations.get(1).cloned());
        Ok(())
    }

    /// A kind above 6 in section 6 is refused as `FieldValue` naming `kind`.
    #[test]
    fn test_an_operation_of_kind_7_is_refused() -> TestResult {
        assert_eq!(
            refused::<ConditionalWriteView>(&conditional([5, 7])?)?,
            ProtocolErrorEnum::FieldValue {
                command: 21,
                field: "kind",
                value: 7
            }
        );
        Ok(())
    }
}
