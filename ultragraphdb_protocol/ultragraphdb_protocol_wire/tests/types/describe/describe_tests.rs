/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::vocab::xsd;
use ultragraphdb_core_mapping::{
    PredicateDeclaration, PredicateKind, RelationDeclaration, TypeDeclaration,
};
use ultragraphdb_protocol_wire::{BodyBuilder, Describe};

use crate::types::write::write_tests::ex;
use crate::utils::golden_bodies::{DESCRIBE_SCHEMA_ID, golden};

pub fn golden_answer() -> Describe {
    Describe::new(0, 42)
        .with_labels(vec!["http://ex.org/Person".to_string()])
        .with_relationship_types(vec!["http://ex.org/knows".to_string()])
        .with_property_keys(vec!["http://ex.org/name".to_string()])
        .with_predicates(vec![
            PredicateDeclaration::new(ex("name"))
                .with_kind(PredicateKind::Property)
                .with_tokenizers(vec!["term".to_string()]),
            PredicateDeclaration::new(ex("knows")).with_kind(PredicateKind::Edge),
        ])
        .with_entity_types(vec![(TypeDeclaration::new(ex("Person"), None, false), 1)])
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
    assert_eq!(answer.namespace(), 0);
    assert_eq!(answer.read_version(), 42);
    assert_eq!(answer.labels(), &["http://ex.org/Person".to_string()]);
    assert_eq!(answer.predicates().len(), 2);
    assert_eq!(answer.entity_types()[0].1, 1);
    assert!(answer.relations().is_empty());
}

#[test]
fn test_the_schema_id_follows_the_declarations_alone() {
    let answer = golden_answer();
    // generate_bodies.py's XXH3-64 of the answer's body without labels, relationship types and property keys.
    assert_eq!(answer.schema_id(), DESCRIBE_SCHEMA_ID);
    let other_data = Describe::new(7, 99)
        .with_labels(vec!["http://ex.org/Other".to_string()])
        .with_predicates(answer.predicates().to_vec())
        .with_entity_types(answer.entity_types().to_vec());
    assert_eq!(other_data.schema_id(), DESCRIBE_SCHEMA_ID);
    let declared =
        golden_answer().with_relations(vec![RelationDeclaration::new(ex("R"), vec![ex("r")])]);
    assert_ne!(declared.schema_id(), DESCRIBE_SCHEMA_ID);
    let typed = golden_answer().with_predicates(vec![
        PredicateDeclaration::new(ex("name"))
            .with_kind(PredicateKind::Property)
            .with_datatype(xsd::STRING.into_owned())
            .with_tokenizers(vec!["term".to_string()]),
        PredicateDeclaration::new(ex("knows")).with_kind(PredicateKind::Edge),
    ]);
    assert_ne!(typed.schema_id(), DESCRIBE_SCHEMA_ID);
}
