/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, DeclaredEntityType, DeclaredKind, DeclaredPredicate, DeclaredRelation,
    Describe, DescribeView,
};

use crate::types::describe::describe_tests::golden_answer;
use crate::utils::golden_bodies::{
    body_data, frame_of, golden, join_sections, put, reframe, split_sections, with_section,
};

fn ex(name: &str) -> String {
    format!("http://ex.org/{name}")
}

#[test]
fn test_the_golden_answer_reads_its_values() {
    let bytes = golden("describe_reply").bytes();
    let view = DescribeView::from_frame(&frame_of(&bytes)).unwrap();
    assert_eq!((view.namespace(), view.read_version()), (0, 42));
    assert_eq!(
        view.labels().collect::<Vec<_>>(),
        vec!["http://ex.org/Person"]
    );
    assert_eq!(
        view.relationship_types().collect::<Vec<_>>(),
        vec!["http://ex.org/knows"]
    );
    assert_eq!(
        view.property_keys().collect::<Vec<_>>(),
        vec!["http://ex.org/name"]
    );
    assert_eq!(view.declarations(), 3);
    assert_eq!(view.strings().len(), 4);
    assert_eq!(view.to_body().unwrap(), golden_answer());
    let request = golden("describe").bytes();
    let view = DescribeView::from_frame(&frame_of(&request)).unwrap();
    assert_eq!(view.to_body().unwrap(), Describe::request(0));
    assert_eq!(view.declarations(), 0);
}

#[test]
fn test_every_declaration_round_trips() {
    let answer = Describe::new(5, 6)
        .with_predicates(vec![
            DeclaredPredicate::new(ex("a"))
                .with_kind(DeclaredKind::Either)
                .with_single(true)
                .with_datatype("http://www.w3.org/2001/XMLSchema#integer")
                .with_unique(true)
                .with_count_index(true)
                .with_tokenizers(vec!["int".to_string()])
                .with_domains(vec![ex("T"), ex("U")])
                .with_vector_index("hnsw", vec![("metric".to_string(), "cosine".to_string())]),
        ])
        .with_entity_types(vec![
            (DeclaredEntityType::new(ex("T"), Some(ex("U")), true), 2),
            (DeclaredEntityType::new(ex("U"), None, false), 1),
        ])
        .with_relations(vec![DeclaredRelation::new(ex("R"), vec![ex("x"), ex("y")])]);
    let bytes = answer.frame(golden("describe").header()).unwrap();
    assert_eq!(
        DescribeView::from_frame(&frame_of(&bytes))
            .unwrap()
            .to_body()
            .unwrap(),
        answer
    );
}

#[test]
fn test_only_declarations_and_names_in_range_are_read() {
    let bytes = with_section(&golden("describe_reply").bytes(), 7, 2, |labels| {
        put(labels, 0, 4, 4)
    });
    assert_eq!(
        DescribeView::from_frame(&frame_of(&bytes)).unwrap_err().0,
        ProtocolErrorEnum::IndexOutOfRange {
            command: 12,
            section: 2,
            at: 0,
            index: 4,
            limit: 4
        }
    );
    // Relationship types (section 3) and property keys (4) index the four strings as labels do.
    for section in [3, 4] {
        let bytes = with_section(&golden("describe_reply").bytes(), 7, section, |names| {
            put(names, 0, 4, 4)
        });
        assert_eq!(
            DescribeView::from_frame(&frame_of(&bytes)).unwrap_err().0,
            ProtocolErrorEnum::IndexOutOfRange {
                command: 12,
                section,
                at: 0,
                index: 4,
                limit: 4
            },
            "section {section}"
        );
    }
    let bytes = with_section(&golden("describe_reply").bytes(), 7, 5, |records| {
        records[0] = 1
    });
    assert_eq!(
        DescribeView::from_frame(&frame_of(&bytes)).unwrap_err().0,
        ProtocolErrorEnum::FieldValue {
            command: 12,
            field: "change kind",
            value: 1
        }
    );
    // A name is text: no IRI syntax is checked here.
    let answer = Describe::new(0, 0).with_labels(vec!["not an iri".to_string()]);
    let bytes = answer.frame(golden("describe").header()).unwrap();
    assert_eq!(
        DescribeView::from_frame(&frame_of(&bytes))
            .unwrap()
            .to_body()
            .unwrap(),
        answer
    );
}

#[test]
fn test_declarations_come_predicates_first_then_entity_types_then_relations() {
    let answer = Describe::new(0, 0)
        .with_predicates(vec![DeclaredPredicate::new(ex("p"))])
        .with_entity_types(vec![(DeclaredEntityType::new(ex("T"), None, false), 1)]);
    let bytes = answer.frame(golden("describe").header()).unwrap();
    let mut secs = split_sections(body_data(&bytes), 0, 7);
    // Records 0 (the predicate, args 0..4) and 1 (the entity type, args 4..6) swapped, their args with them.
    let mut entity = secs[5][16..].to_vec();
    let mut predicate = secs[5][..16].to_vec();
    let args: Vec<u32> = secs[6]
        .chunks(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    entity[4..8].copy_from_slice(&0u32.to_le_bytes());
    predicate[4..8].copy_from_slice(&2u32.to_le_bytes());
    secs[5] = [entity, predicate].concat();
    secs[6] = args[4..]
        .iter()
        .chain(&args[..4])
        .flat_map(|a| a.to_le_bytes())
        .collect();
    let swapped = reframe(&bytes, &join_sections(&[], &secs));
    assert_eq!(
        DescribeView::from_frame(&frame_of(&swapped)).unwrap_err().0,
        ProtocolErrorEnum::FieldValue {
            command: 12,
            field: "declaration order",
            value: 8
        }
    );
}
