/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyBuilder, BodyView, DropCommand, DropCommandView, DropKind};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::commit::commit_tests::session;
use crate::types::drop_command::drop_command_tests::{golden_drop, golden_drop_reply};
use crate::utils::golden_bodies::{frame_of, golden, put, with_command_header};

#[test]
fn test_the_golden_drop_reads_its_values() -> TestResult {
    let bytes = golden("drop")?.bytes()?;
    let view = DropCommandView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(view.kind(), DropKind::Graph);
    assert_eq!(view.tenant(), 3);
    assert_eq!(view.target(), Some("http://ex.org/g"));
    assert_eq!(
        (view.session(), view.request(), view.commit_version()),
        (session(), 8, 0)
    );
    assert_eq!(view.token(), None);
    assert_eq!(view.to_body()?, golden_drop());
    let reply = golden("drop_reply")?.bytes()?;
    let reply_view = DropCommandView::from_frame(&frame_of(&reply)?)?;
    assert_eq!(reply_view.commit_version(), (31 << 22) | 2);
    assert_eq!(reply_view.to_body()?, golden_drop_reply()?);
    // A reply's token names its commit version's epoch.
    let wrong = with_command_header(&reply, |h| put(h, 64, 30, 8));
    assert_eq!(
        DropCommandView::from_frame(&frame_of(&wrong)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::FieldValue {
            command: 19,
            field: "token",
            value: 30
        }
    );
    Ok(())
}

fn refused(change: impl FnOnce(&mut [u8])) -> TestResult<ProtocolErrorEnum> {
    let bytes = with_command_header(&golden("drop")?.bytes()?, change);
    Ok(DropCommandView::from_frame(&frame_of(&bytes)?)
        .err()
        .context("a refusal")?
        .0)
}

#[test]
fn test_unused_bytes_kind_and_tenant_are_checked_in_order() -> TestResult {
    // PROTOCOL.md section 6.5: bytes 72 to 79 hold `catalog`; 1 to 7 and 80 to 127 are unused.
    for at in [1usize, 7, 80, 127] {
        assert_eq!(
            refused(|h| {
                h[at] = 1;
                h[0] = 9;
            })?,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    for kind in [0u8, 6, 255] {
        assert_eq!(
            refused(|h| h[0] = kind)?,
            ProtocolErrorEnum::FieldValue {
                command: 19,
                field: "kind",
                value: kind.into()
            }
        );
    }
    Ok(())
}

fn request_refused(value: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command: 19,
        field: "request",
        value,
    }
}

#[test]
fn test_a_session_with_request_0_is_refused() -> TestResult {
    assert_eq!(refused(|h| h[32..40].fill(0))?, request_refused(0));
    Ok(())
}

#[test]
fn test_no_session_with_a_request_is_refused() -> TestResult {
    let bytes = golden("drop")?.bytes()?;
    let request = DropCommandView::from_frame(&frame_of(&bytes)?)?.request();
    assert_eq!(refused(|h| h[16..32].fill(0))?, request_refused(request));
    Ok(())
}

#[test]
fn test_no_session_with_request_0_is_read() -> TestResult {
    let bytes = with_command_header(&golden("drop")?.bytes()?, |h| h[16..40].fill(0));
    let view = DropCommandView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!((view.session(), view.request()), ([0; 16], 0));
    Ok(())
}

/// `request` comes after the unused bytes and before `kind`.
#[test]
fn test_the_request_lies_between_the_unused_bytes_and_the_kind() -> TestResult {
    assert_eq!(
        refused(|h| {
            h[1] = 1;
            h[32..40].fill(0);
        })?,
        ProtocolErrorEnum::ReservedNotZero { offset: 128 + 1 }
    );
    assert_eq!(
        refused(|h| {
            h[32..40].fill(0);
            h[0] = 9;
        })?,
        request_refused(0)
    );
    Ok(())
}

#[test]
fn test_a_predicate_and_a_graph_name_one_iri_and_the_others_none() -> TestResult {
    // A tenant's data drop under the golden graph drop's term table: one term too many.
    assert_eq!(
        refused(|h| h[0] = 3)?,
        ProtocolErrorEnum::CountMismatch {
            command: 19,
            field: "drop terms",
            count: 1,
            expected: 0
        }
    );
    // A graph drop with no term.
    let data = DropCommand::tenant_data(3).frame(golden("drop")?.header())?;
    let graph = with_command_header(&data, |h| h[0] = 2);
    assert_eq!(
        DropCommandView::from_frame(&frame_of(&graph)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::CountMismatch {
            command: 19,
            field: "drop terms",
            count: 0,
            expected: 1
        }
    );
    // A predicate named by a literal, not an IRI.
    let mut terms = ultragraphdb_protocol_wire::TermTable::new();
    let first = terms.intern(ultragraphdb_protocol_wire::RequestTerm::Literal {
        lexical: "p".into(),
        datatype: None,
    })?;
    assert_eq!(first, 0);
    let literal = ultragraphdb_protocol_wire::Write::new(terms, Vec::new())
        .frame(golden("drop")?.header())?;
    // The Write's sections 2 and 3 hold the table; a Drop's are 0 and 1.
    let secs = crate::utils::golden_bodies::split_sections(
        crate::utils::golden_bodies::body_data(&literal),
        0,
        6,
    );
    let drop = crate::utils::golden_bodies::reframe(
        &golden("drop")?.bytes()?,
        &crate::utils::golden_bodies::join_sections(&[], &secs[2..4]),
    );
    assert_eq!(
        DropCommandView::from_frame(&frame_of(&drop)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::FieldValue {
            command: 19,
            field: "drop term",
            value: 3
        }
    );
    // Each kind round-trips through its view.
    for command in [
        DropCommand::predicate(1, "http://ex.org/p").with_session([2; 16], 3),
        DropCommand::tenant_data(4),
        DropCommand::whole_tenant(5),
        golden_drop_reply()?,
    ] {
        let bytes = command.frame(golden("drop")?.header())?;
        assert_eq!(
            DropCommandView::from_frame(&frame_of(&bytes)?)?.to_body()?,
            command
        );
    }
    Ok(())
}

/// The golden graph drop with its term table replaced by one holding `term` alone.
fn drop_naming(term: ultragraphdb_protocol_wire::RequestTerm) -> TestResult<Vec<u8>> {
    use crate::utils::golden_bodies::{body_data, join_sections, reframe, split_sections};
    let mut terms = ultragraphdb_protocol_wire::TermTable::new();
    assert_eq!(terms.intern(term)?, 0, "the table's only term");
    let write = ultragraphdb_protocol_wire::Write::new(terms, Vec::new())
        .frame(golden("drop")?.header())?;
    // The Write's sections 2 and 3 hold the table; a Drop's are 0 and 1.
    let secs = split_sections(body_data(&write), 0, 6);
    Ok(reframe(
        &golden("drop")?.bytes()?,
        &join_sections(&[], &secs[2..4]),
    ))
}

#[test]
fn test_a_drop_target_of_any_kind_but_an_iri_is_refused_naming_its_kind() -> TestResult {
    use ultragraphdb_protocol_wire::{Direction, RequestTerm};
    // PROTOCOL.md section 6.5: the entry is an IRI (`FieldValue` `"drop term"`, the entry's kind: 2 a blank node,
    // 4 a language-tagged literal, section 4.4). A triple term cannot be the table's one entry: it names three
    // entries before its own, so such a table is refused by its count first.
    let cases = [
        (RequestTerm::BlankNode("g".into()), 2u64),
        (
            RequestTerm::LanguageLiteral {
                lexical: "g".into(),
                language: "en".into(),
                direction: Some(Direction::Ltr),
            },
            4,
        ),
    ];
    for (term, kind) in cases {
        assert_eq!(
            DropCommandView::from_frame(&frame_of(&drop_naming(term)?)?)
                .err()
                .context("a refusal")?
                .0,
            ProtocolErrorEnum::FieldValue {
                command: 19,
                field: "drop term",
                value: kind
            }
        );
    }
    // An IRI is taken.
    let iri = drop_naming(RequestTerm::Iri("http://ex.org/h".into()))?;
    assert_eq!(
        DropCommandView::from_frame(&frame_of(&iri)?)?.target(),
        Some("http://ex.org/h")
    );
    Ok(())
}

#[test]
fn test_a_reply_through_no_session_holds_the_zero_token() -> TestResult {
    let header = golden("drop_reply")?.header();
    let version = (31 << 22) | 2;
    let reply = DropCommand::tenant_data(3).with_commit_version(version);
    let bytes = reply.clone().frame(header.clone())?;
    let view = DropCommandView::from_frame(&frame_of(&bytes)?)?;
    assert!(view.token().is_none());
    assert_eq!(view.to_body()?, reply);
    let token = with_command_header(&bytes, |h| {
        put(h, 48, 2, 4);
        put(h, 56, 42, 8);
        put(h, 64, 31, 8);
    });
    assert_eq!(
        DropCommandView::from_frame(&frame_of(&token)?)
            .err()
            .context("a refusal")?
            .0,
        ProtocolErrorEnum::FieldValue {
            command: 19,
            field: "token",
            value: 31
        }
    );
    Ok(())
}

/// Stage 6.6: Drop's `catalog` (`PROTOCOL.md` §6.5): the catalog version a drop of kind 3 or 4 drops under, 0 for kinds
/// 1 and 2.
mod stage_6 {
    use ultragraphdb_error_protocol::ProtocolErrorEnum;
    use ultragraphdb_protocol_wire::{
        BodyBuilder, BodyView, Command, DropCommand, DropCommandView, Header,
    };
    use ultragraphdb_tool_bdd::TestResult;

    use crate::utils::golden_bodies::{CLUSTER, frame_of, golden, with_command_header};
    use crate::utils::ugs_frames::{frame, refused};

    fn catalog_named(catalog: u64) -> ProtocolErrorEnum {
        ProtocolErrorEnum::FieldValue {
            command: 19,
            field: "catalog",
            value: catalog,
        }
    }

    /// wire-protocol, "A Drop of a predicate names no catalog version": kind 1 with `catalog` 7 is refused, and kind 2
    /// likewise.
    #[test]
    fn test_a_drop_of_a_predicate_or_graph_names_no_catalog_version() -> TestResult {
        let graph = golden("drop")?.bytes()?;
        let predicate = with_command_header(&graph, |h| {
            h[0] = 1;
            h[72] = 7;
        });
        assert_eq!(refused::<DropCommandView>(&predicate)?, catalog_named(7));
        let graph_catalog = with_command_header(&graph, |h| h[72] = 7);
        assert_eq!(
            refused::<DropCommandView>(&graph_catalog)?,
            catalog_named(7)
        );
        Ok(())
    }

    /// A drop of a tenant's data names its catalog version at offset 72, built and read back; one of everything in a
    /// tenant holding none names 0.
    #[test]
    fn test_a_drop_of_a_tenants_data_names_its_catalog_version() -> TestResult {
        let drop = DropCommand::tenant_data(6).with_catalog(7);
        assert_eq!(drop.catalog(), 7);
        let expected = frame(
            Command::Drop,
            &[
                (0, &[3]),
                (8, &6u64.to_le_bytes()),
                (72, &7u64.to_le_bytes()),
            ],
            &[vec![], vec![]],
            false,
        )?;
        let header = Header::new(CLUSTER, Command::Drop).with_stream(3);
        let bytes = drop.frame(header)?;
        assert_eq!(bytes, expected);
        let view = DropCommandView::from_frame(&frame_of(&bytes)?)?;
        assert_eq!(view.catalog(), 7);
        assert_eq!(view.to_body()?.catalog(), 7);
        let whole = frame(
            Command::Drop,
            &[(0, &[4]), (8, &3u64.to_le_bytes())],
            &[vec![], vec![]],
            false,
        )?;
        assert_eq!(
            DropCommandView::from_frame(&frame_of(&whole)?)?.catalog(),
            0
        );
        Ok(())
    }
}
