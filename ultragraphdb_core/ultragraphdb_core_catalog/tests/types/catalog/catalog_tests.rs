/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;
use oxrdf::vocab::rdf;
use ultragraphdb_core_catalog::{Catalog, CatalogErrorEnum};
use ultragraphdb_core_mapping::{
    Declaration, PredicateDeclaration, PredicateKind, RelationDeclaration, TypeDeclaration,
    VectorIndexSpec,
};

fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

#[test]
fn test_the_initial_catalog_declares_rdf_type_an_edge_and_nothing_else() {
    let catalog = Catalog::initial(4, 1);
    assert_eq!(catalog.namespace(), 4);
    assert_eq!(catalog.predicates().count(), 1);
    let declared = catalog.predicate(1).unwrap();
    assert_eq!(
        (declared.predicate(), declared.kind()),
        (&rdf::TYPE.into_owned(), PredicateKind::Edge)
    );
    assert_eq!(
        catalog.entity_types().count() + catalog.relations().count(),
        0
    );
}

#[test]
fn test_an_undeclared_predicate_behaves_as_the_default() {
    let catalog = Catalog::new(0);
    let default = catalog.predicate_or_default(9, &ex("nick"));
    assert_eq!(*default, PredicateDeclaration::new(ex("nick")));
    assert_eq!(default.kind(), PredicateKind::Either);
    assert!(!default.unique() && default.tokenizers().is_empty() && default.datatype().is_none());
}

#[test]
fn test_a_declaration_with_unknown_indexes_is_refused_and_a_valid_one_replaces() {
    let mut catalog = Catalog::new(0);
    let unknown = PredicateDeclaration::new(ex("p")).with_tokenizers(vec!["nope".into()]);
    assert!(matches!(
        catalog.declare_predicate(2, unknown).unwrap_err().0,
        CatalogErrorEnum::UnknownTokenizer { .. }
    ));
    let vector =
        PredicateDeclaration::new(ex("p")).with_vector_index(VectorIndexSpec::new("nope", vec![]));
    assert!(matches!(
        catalog.declare_predicate(2, vector).unwrap_err().0,
        CatalogErrorEnum::UnknownVectorIndex { .. }
    ));
    assert!(catalog.predicate(2).is_none());
    let hnsw =
        PredicateDeclaration::new(ex("p")).with_vector_index(VectorIndexSpec::new("hnsw", vec![]));
    catalog.declare_predicate(2, hnsw.clone()).unwrap();
    let exact = PredicateDeclaration::new(ex("p")).with_tokenizers(vec!["exact".into()]);
    catalog
        .declare(2, Declaration::Predicate(exact.clone()), None)
        .unwrap();
    assert_eq!(catalog.predicate(2), Some(&exact));
    assert!(catalog.remove_predicate(2) && !catalog.remove_predicate(2));
    catalog
        .declare(
            3,
            Declaration::Relation(RelationDeclaration::new(ex("R"), vec![ex("role")])),
            None,
        )
        .unwrap();
    catalog
        .declare(
            4,
            Declaration::EntityType(TypeDeclaration::new(ex("T"), None, true)),
            None,
        )
        .unwrap();
    assert!(catalog.relation(3).is_some() && catalog.entity_type(4).unwrap().is_abstract());
}
