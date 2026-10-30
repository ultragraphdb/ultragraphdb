/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyBuilder, BodyView, DropCommand, DropCommandView, DropKind};

use crate::types::drop_command::drop_command_tests::golden_drop;
use crate::utils::golden_bodies::{frame_of, golden, put, with_command_header};

#[test]
fn test_the_golden_drop_reads_its_values() {
    let bytes = golden("drop").bytes();
    let view = DropCommandView::from_frame(&frame_of(&bytes)).unwrap();
    assert_eq!(view.kind(), DropKind::Graph);
    assert_eq!(view.namespace(), 3);
    assert_eq!(view.target(), Some("http://ex.org/g"));
    assert_eq!(
        (view.session(), view.request(), view.commit_version()),
        (9, 8, 0)
    );
    assert_eq!(view.to_body().unwrap(), golden_drop());
    let reply = golden("drop_reply").bytes();
    let view = DropCommandView::from_frame(&frame_of(&reply)).unwrap();
    assert_eq!(view.commit_version(), 2002);
}

fn refused(change: impl FnOnce(&mut [u8])) -> ProtocolErrorEnum {
    let bytes = with_command_header(&golden("drop").bytes(), change);
    DropCommandView::from_frame(&frame_of(&bytes))
        .unwrap_err()
        .0
}

#[test]
fn test_unused_bytes_kind_and_namespace_are_checked_in_order() {
    for at in [1usize, 7, 40, 127] {
        assert_eq!(
            refused(|h| {
                h[at] = 1;
                h[0] = 9;
            }),
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    for kind in [0u8, 6, 255] {
        assert_eq!(
            refused(|h| h[0] = kind),
            ProtocolErrorEnum::FieldValue {
                command: 19,
                field: "kind",
                value: kind.into()
            }
        );
    }
    // Every namespace is named by no namespace.
    let all = DropCommand::all().frame(golden("drop").header()).unwrap();
    let named = with_command_header(&all, |h| put(h, 8, 3, 8));
    assert_eq!(
        DropCommandView::from_frame(&frame_of(&named))
            .unwrap_err()
            .0,
        ProtocolErrorEnum::FieldValue {
            command: 19,
            field: "namespace",
            value: 3
        }
    );
}

#[test]
fn test_a_predicate_and_a_graph_name_one_iri_and_the_others_none() {
    // A namespace's data drop under the golden graph drop's term table: one term too many.
    assert_eq!(
        refused(|h| h[0] = 3),
        ProtocolErrorEnum::CountMismatch {
            command: 19,
            field: "drop terms",
            count: 1,
            expected: 0
        }
    );
    // A graph drop with no term.
    let data = DropCommand::namespace_data(3)
        .frame(golden("drop").header())
        .unwrap();
    let graph = with_command_header(&data, |h| h[0] = 2);
    assert_eq!(
        DropCommandView::from_frame(&frame_of(&graph))
            .unwrap_err()
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
    terms
        .intern(ultragraphdb_protocol_wire::RequestTerm::Literal {
            lexical: "p".into(),
            datatype: None,
        })
        .unwrap();
    let literal = ultragraphdb_protocol_wire::Write::new(terms, Vec::new())
        .frame(golden("drop").header())
        .unwrap();
    // The Write's sections 2 and 3 hold the table; a Drop's are 0 and 1.
    let secs = crate::utils::golden_bodies::split_sections(
        crate::utils::golden_bodies::body_data(&literal),
        0,
        6,
    );
    let drop = crate::utils::golden_bodies::reframe(
        &golden("drop").bytes(),
        &crate::utils::golden_bodies::join_sections(&[], &secs[2..4]),
    );
    assert_eq!(
        DropCommandView::from_frame(&frame_of(&drop)).unwrap_err().0,
        ProtocolErrorEnum::FieldValue {
            command: 19,
            field: "drop term",
            value: 3
        }
    );
    // Each kind round-trips through its view.
    for drop in [
        DropCommand::predicate(1, "http://ex.org/p").with_session(2, 3),
        DropCommand::namespace_data(4),
        DropCommand::whole_namespace(5),
        DropCommand::all().with_commit_version(6),
    ] {
        let bytes = drop.frame(golden("drop").header()).unwrap();
        assert_eq!(
            DropCommandView::from_frame(&frame_of(&bytes))
                .unwrap()
                .to_body()
                .unwrap(),
            drop
        );
    }
}

/// The golden graph drop with its term table replaced by one holding `term` alone.
fn drop_naming(term: ultragraphdb_protocol_wire::RequestTerm) -> Vec<u8> {
    use crate::utils::golden_bodies::{body_data, join_sections, reframe, split_sections};
    let mut terms = ultragraphdb_protocol_wire::TermTable::new();
    terms.intern(term).unwrap();
    let write = ultragraphdb_protocol_wire::Write::new(terms, Vec::new())
        .frame(golden("drop").header())
        .unwrap();
    // The Write's sections 2 and 3 hold the table; a Drop's are 0 and 1.
    let secs = split_sections(body_data(&write), 0, 6);
    reframe(&golden("drop").bytes(), &join_sections(&[], &secs[2..4]))
}

#[test]
fn test_a_drop_target_of_any_kind_but_an_iri_is_refused_naming_its_kind() {
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
            DropCommandView::from_frame(&frame_of(&drop_naming(term)))
                .unwrap_err()
                .0,
            ProtocolErrorEnum::FieldValue {
                command: 19,
                field: "drop term",
                value: kind
            }
        );
    }
    // An IRI is taken.
    let iri = drop_naming(RequestTerm::Iri("http://ex.org/h".into()));
    assert_eq!(
        DropCommandView::from_frame(&frame_of(&iri))
            .unwrap()
            .target(),
        Some("http://ex.org/h")
    );
}
