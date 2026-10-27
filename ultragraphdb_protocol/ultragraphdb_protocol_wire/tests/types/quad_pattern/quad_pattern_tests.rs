/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, ColumnType, PatternSlot, Prepare, PrepareView, QuadPattern, Read,
    ReadView,
};

use crate::utils::golden_bodies::{
    body_data, frame_of, golden, put, reframe, section, with_section,
};

fn p(s: &str) -> PatternSlot<String> {
    PatternSlot::Bound(s.to_string())
}

#[test]
fn test_a_pattern_keeps_its_slots_and_counts_parameters() {
    let pattern = QuadPattern::new(
        PatternSlot::DefaultGraph,
        PatternSlot::Parameter(0),
        p("<http://ex.org/name>"),
        PatternSlot::Parameter(1),
    );
    assert_eq!(pattern.graph(), &PatternSlot::DefaultGraph);
    assert_eq!(pattern.subject(), &PatternSlot::Parameter(0));
    assert_eq!(pattern.predicate(), &p("<http://ex.org/name>"));
    assert_eq!(pattern.object(), &PatternSlot::Parameter(1));
    assert_eq!(pattern.parameters(), 2);
    assert_eq!(pattern.slots()[2], &p("<http://ex.org/name>"));
    assert_eq!(
        QuadPattern::<String>::slot_types(),
        [
            ColumnType::Node,
            ColumnType::Node,
            ColumnType::Node,
            ColumnType::Term
        ]
    );
    let borrowed = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Bound("<a>"),
        PatternSlot::Unbound,
        PatternSlot::Unbound,
    );
    assert_eq!(borrowed.to_owned_pattern().subject(), &p("<a>"));
    assert_eq!(borrowed.parameters(), 0);
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
    // The golden Read has two terms.
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
}

#[test]
fn test_bound_terms_follow_their_slots_and_are_all_bound() {
    // The golden Read binds the subject to term 0 and the predicate to term 1; swapped, the subject names term 1.
    assert_eq!(
        read_with([(0, 0), (2, 1), (2, 0), (0, 0)]),
        ProtocolErrorEnum::FieldValue {
            command: 7,
            field: "term index",
            value: 1
        }
    );
    // One bound slot for two terms: a term no slot binds.
    assert_eq!(
        read_with([(0, 0), (2, 0), (0, 0), (0, 0)]),
        ProtocolErrorEnum::CountMismatch {
            command: 7,
            field: "pattern terms",
            count: 2,
            expected: 1
        }
    );
}

#[test]
fn test_parameters_are_numbered_in_order_of_appearance() {
    let prepare =
        |subject: PatternSlot<String>, object: PatternSlot<String>, types: Vec<ColumnType>| {
            let pattern = QuadPattern::new(PatternSlot::Unbound, subject, p("<p>"), object);
            let bytes = Prepare::new(0, 0, 0, pattern, types, Vec::new())
                .frame(golden("prepare").header())
                .unwrap();
            PrepareView::from_frame(&frame_of(&bytes)).map(|v| v.pattern().parameters())
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
        .unwrap_err()
        .0,
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
        .unwrap_err()
        .0,
        ProtocolErrorEnum::FieldValue {
            command: 10,
            field: "parameter",
            value: 0
        }
    );
    // A Read refuses any parameter; Read::frame of one is refused when read.
    let read = Read::new(
        0,
        0,
        QuadPattern::new(
            PatternSlot::Unbound,
            PatternSlot::Parameter(0),
            p("<p>"),
            PatternSlot::Unbound,
        ),
        Vec::new(),
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
