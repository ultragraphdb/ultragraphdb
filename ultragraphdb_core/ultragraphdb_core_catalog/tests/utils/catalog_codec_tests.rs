/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;
use ultragraphdb_core_catalog::{decode_declaration, encode_declaration};
use ultragraphdb_core_mapping::{
    Cardinality, Declaration, PredicateDeclaration, PredicateKind, RelationDeclaration,
    TypeDeclaration, VectorIndexSpec,
};

fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

#[test]
fn test_every_declaration_round_trips_and_every_truncation_is_refused() {
    let declarations = vec![
        Declaration::Predicate(PredicateDeclaration::new(ex("plain"))),
        Declaration::Predicate(
            PredicateDeclaration::new(ex("full"))
                .with_kind(PredicateKind::Property)
                .with_cardinality(Cardinality::Single)
                .with_datatype(ex("dt"))
                .with_unique(true)
                .with_tokenizers(vec!["exact".into(), "term".into()])
                .with_count_index(true)
                .with_vector_index(VectorIndexSpec::new(
                    "hnsw",
                    vec![("metric".into(), "cosine".into())],
                ))
                .with_domains(vec![ex("Person"), ex("Org")]),
        ),
        Declaration::Predicate(PredicateDeclaration::new(ex("e")).with_kind(PredicateKind::Edge)),
        Declaration::EntityType(TypeDeclaration::new(ex("T"), Some(ex("P")), true)),
        Declaration::EntityType(TypeDeclaration::new(ex("U"), None, false)),
        Declaration::Relation(RelationDeclaration::new(ex("R"), vec![ex("a"), ex("b")])),
    ];
    for declaration in declarations {
        let bytes = encode_declaration(&declaration);
        assert_eq!(decode_declaration(b"k", &bytes), Ok(declaration.clone()));
        for end in 0..bytes.len() {
            assert!(
                decode_declaration(b"k", &bytes[..end]).is_err(),
                "{declaration:?} cut at {end}"
            );
        }
        let mut longer = bytes.clone();
        longer.push(0);
        assert!(decode_declaration(b"k", &longer).is_err());
    }
}

#[test]
fn test_unknown_codes_are_refused() {
    let bytes = encode_declaration(&Declaration::Predicate(PredicateDeclaration::new(ex("p"))));
    let kind_at = 2 + 4 + "http://ex.org/p".len();
    for (at, value) in [(1, 9u8), (kind_at, 9), (kind_at + 1, 9)] {
        let mut bad = bytes.clone();
        bad[at] = value;
        assert!(
            decode_declaration(b"k", &bad).is_err(),
            "byte {at} = {value}"
        );
    }
}
