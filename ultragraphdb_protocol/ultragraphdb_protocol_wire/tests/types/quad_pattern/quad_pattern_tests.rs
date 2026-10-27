/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, ColumnType, PatternSlot, Prepare, PrepareView, QuadPattern, Read,
    ReadView, TermTable,
};

use crate::utils::golden_bodies::{
    body_data, frame_of, golden, put, reframe, section, with_section,
};

#[test]
fn test_a_pattern_keeps_its_slots_and_counts_parameters() {
    let pattern = QuadPattern::new(
        PatternSlot::DefaultGraph,
        PatternSlot::Parameter(0),
        PatternSlot::Bound(3),
        PatternSlot::Parameter(1),
    );
    assert_eq!(pattern.graph(), PatternSlot::DefaultGraph);
    assert_eq!(pattern.subject(), PatternSlot::Parameter(0));
    assert_eq!(pattern.predicate(), PatternSlot::Bound(3));
    assert_eq!(pattern.object(), PatternSlot::Parameter(1));
    assert_eq!(pattern.parameters(), 2);
    assert_eq!(pattern.slots()[2], PatternSlot::Bound(3));
    assert_eq!(
        QuadPattern::slot_types(),
        [
            ColumnType::Node,
            ColumnType::Node,
            ColumnType::Node,
            ColumnType::Term
        ]
    );
}

fn read_with(slots: [(u32, u32); 4]) -> ProtocolErrorEnum {
    let bytes = golden("read").bytes();
    let mut data = body_data(&bytes).to_vec();
    let (at, _) = section(&data, 0, 0);
    for (i, (kind, value)) in slots.iter().enumerate() {
        put(&mut data, at + 8 * i, (*kind).into(), 4);
        put(&mut data, at + 8 * i + 4, (*value).into(), 4);
    }
    ReadView::from_frame(&frame_of(&reframe(&bytes, &data)))
        .unwrap_err()
        .0
}

#[test]
fn test_slot_kinds_values_and_places_are_checked() {
    // The golden Read's table holds two terms.
    assert_eq!(
        read_with([(4, 0), (0, 0), (0, 0), (0, 0)]),
        ProtocolErrorEnum::FieldValue {
            command: 7,
            field: "slot kind",
            value: 4
        }
    );
    assert_eq!(
        read_with([(0, 1), (0, 0), (0, 0), (0, 0)]),
        ProtocolErrorEnum::FieldValue {
            command: 7,
            field: "slot value",
            value: 1
        }
    );
    assert_eq!(
        read_with([(0, 0), (1, 0), (0, 0), (0, 0)]),
        ProtocolErrorEnum::FieldValue {
            command: 7,
            field: "default graph",
            value: 1
        }
    );
    assert_eq!(
        read_with([(0, 0), (3, 0), (0, 0), (0, 0)]),
        ProtocolErrorEnum::FieldValue {
            command: 7,
            field: "parameter slot",
            value: 1
        }
    );
    assert_eq!(
        read_with([(0, 0), (2, 2), (0, 0), (0, 0)]),
        ProtocolErrorEnum::IndexOutOfRange {
            command: 7,
            section: 0,
            at: 1,
            index: 2,
            limit: 2
        }
    );
    let short = with_section(&golden("read").bytes(), 4, 0, |pattern| {
        pattern.truncate(24)
    });
    assert_eq!(
        ReadView::from_frame(&frame_of(&short)).unwrap_err().0,
        ProtocolErrorEnum::CountMismatch {
            command: 7,
            field: "pattern",
            count: 24,
            expected: 32
        }
    );
    // Terms bound in any order, the same term twice, read.
    let mut terms = TermTable::new();
    let (a, b) = (terms.iri("a"), terms.iri("b"));
    let pattern = QuadPattern::new(
        PatternSlot::Bound(b),
        PatternSlot::Bound(a),
        PatternSlot::Bound(b),
        PatternSlot::Unbound,
    );
    let bytes = Read::new(0, 0, terms, pattern, vec![])
        .frame(golden("read").header())
        .unwrap();
    assert_eq!(
        *ReadView::from_frame(&frame_of(&bytes)).unwrap().pattern(),
        pattern
    );
}

#[test]
fn test_parameters_are_numbered_in_order_of_appearance() {
    let prepare = |subject: PatternSlot, object: PatternSlot, types: Vec<ColumnType>| {
        let mut terms = TermTable::new();
        let p = terms.iri("p");
        let pattern =
            QuadPattern::new(PatternSlot::Unbound, subject, PatternSlot::Bound(p), object);
        let bytes = Prepare::new(0, 0, 0, terms, pattern, types, Vec::new())
            .frame(golden("prepare").header())
            .unwrap();
        PrepareView::from_frame(&frame_of(&bytes))
            .map(|v| v.pattern().parameters())
            .map_err(|e| e.0)
    };
    assert_eq!(
        prepare(
            PatternSlot::Parameter(0),
            PatternSlot::Parameter(1),
            vec![ColumnType::Node, ColumnType::Term]
        ),
        Ok(2)
    );
    assert_eq!(
        prepare(
            PatternSlot::Parameter(1),
            PatternSlot::Parameter(0),
            vec![ColumnType::Node, ColumnType::Term]
        )
        .unwrap_err(),
        ProtocolErrorEnum::FieldValue {
            command: 10,
            field: "parameter",
            value: 1
        }
    );
    assert_eq!(
        prepare(
            PatternSlot::Parameter(0),
            PatternSlot::Parameter(0),
            vec![ColumnType::Node]
        )
        .unwrap_err(),
        ProtocolErrorEnum::FieldValue {
            command: 10,
            field: "parameter",
            value: 0
        }
    );
    let mut terms = TermTable::new();
    let p = terms.iri("p");
    let read = Read::new(
        0,
        0,
        terms,
        QuadPattern::new(
            PatternSlot::Unbound,
            PatternSlot::Parameter(0),
            PatternSlot::Bound(p),
            PatternSlot::Unbound,
        ),
        vec![],
    )
    .frame(golden("read").header())
    .unwrap();
    assert_eq!(
        ReadView::from_frame(&frame_of(&read)).unwrap_err().0,
        ProtocolErrorEnum::FieldValue {
            command: 7,
            field: "parameter slot",
            value: 1
        }
    );
}
