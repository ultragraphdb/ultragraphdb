/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{
    BodyBuilder, DeclaredEntityType, DeclaredKind, DeclaredPredicate, DeclaredRelation, Describe,
};

use crate::utils::golden_bodies::{DESCRIBE_SCHEMA_ID, golden};

fn ex(name: &str) -> String {
    format!("http://ex.org/{name}")
}

pub fn golden_answer() -> Describe {
    Describe::new(0, 42)
        .with_labels(vec![ex("Person")])
        .with_relationship_types(vec![ex("knows")])
        .with_property_keys(vec![ex("name")])
        .with_predicates(vec![
            DeclaredPredicate::new(ex("name"))
                .with_kind(DeclaredKind::Property)
                .with_tokenizers(vec!["term".to_string()]),
            DeclaredPredicate::new(ex("knows")).with_kind(DeclaredKind::Edge),
        ])
        .with_entity_types(vec![(
            DeclaredEntityType::new(ex("Person"), None, false),
            1,
        )])
}

#[test]
fn test_the_request_and_the_answer_build_the_golden_frames() {
    assert_eq!(
        Describe::request(0)
            .frame(golden("describe").header())
            .unwrap(),
        golden("describe").bytes()
    );
    assert_eq!(Describe::request(0), Describe::new(0, 0));
    let answer = golden_answer();
    assert_eq!(
        answer.frame(golden("describe_reply").header()).unwrap(),
        golden("describe_reply").bytes()
    );
    assert_eq!((answer.namespace(), answer.read_version()), (0, 42));
    assert_eq!(answer.labels(), &[ex("Person")]);
    assert_eq!(answer.predicates().len(), 2);
    assert_eq!(answer.entity_types()[0].1, 1);
    assert!(answer.relations().is_empty());
}

#[test]
fn test_the_schema_id_follows_the_declarations_alone() {
    let answer = golden_answer();
    assert_eq!(answer.schema_id(), DESCRIBE_SCHEMA_ID);
    let other_data = Describe::new(7, 99)
        .with_labels(vec![ex("Other")])
        .with_predicates(answer.predicates().to_vec())
        .with_entity_types(answer.entity_types().to_vec());
    assert_eq!(other_data.schema_id(), DESCRIBE_SCHEMA_ID);
    let declared =
        golden_answer().with_relations(vec![DeclaredRelation::new(ex("R"), vec![ex("r")])]);
    assert_ne!(declared.schema_id(), DESCRIBE_SCHEMA_ID);
    let typed = golden_answer().with_predicates(vec![
        DeclaredPredicate::new(ex("name"))
            .with_kind(DeclaredKind::Property)
            .with_datatype("http://www.w3.org/2001/XMLSchema#string")
            .with_tokenizers(vec!["term".to_string()]),
        DeclaredPredicate::new(ex("knows")).with_kind(DeclaredKind::Edge),
    ]);
    assert_ne!(typed.schema_id(), DESCRIBE_SCHEMA_ID);
}
