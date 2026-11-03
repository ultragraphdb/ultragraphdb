/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;
use oxrdf::vocab::rdf;
use ultragraphdb_core_catalog::{Catalog, CatalogErrorEnum};
use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_mapping::{
    Declaration, PredicateDeclaration, PredicateKind, RelationDeclaration, TypeDeclaration,
    VectorIndexSpec,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// The ID of sequence `n` in bucket 0, of the built-in `Resource` type.
fn id(n: u64) -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::RESOURCE,
        Bucket::new(0)?,
        Sequence::new(n)?,
    ))
}

fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

#[test]
fn test_the_initial_catalog_declares_rdf_type_an_edge_and_nothing_else() -> TestResult {
    let catalog = Catalog::initial(4, id(1)?);
    assert_eq!(catalog.namespace(), 4);
    assert_eq!(catalog.predicates().count(), 1);
    let declared = catalog.predicate(id(1)?).context("rdf:type is declared")?;
    assert_eq!(
        (declared.predicate(), declared.kind()),
        (&rdf::TYPE.into_owned(), PredicateKind::Edge)
    );
    assert_eq!(
        catalog.entity_types().count() + catalog.relations().count(),
        0
    );
    Ok(())
}

#[test]
fn test_an_undeclared_predicate_behaves_as_the_default() -> TestResult {
    let catalog = Catalog::new(0);
    let default = catalog.predicate_or_default(id(9)?, &ex("nick"));
    assert_eq!(*default, PredicateDeclaration::new(ex("nick")));
    assert_eq!(default.kind(), PredicateKind::Either);
    assert!(!default.unique() && default.tokenizers().is_empty() && default.datatype().is_none());
    Ok(())
}

#[test]
fn test_a_declaration_with_unknown_indexes_is_refused_and_a_valid_one_replaces() -> TestResult {
    let mut catalog = Catalog::new(0);
    let unknown = PredicateDeclaration::new(ex("p")).with_tokenizers(vec!["nope".into()]);
    assert!(matches!(
        catalog.declare_predicate(id(2)?, unknown).map_err(|e| e.0),
        Err(CatalogErrorEnum::UnknownTokenizer { .. })
    ));
    let vector =
        PredicateDeclaration::new(ex("p")).with_vector_index(VectorIndexSpec::new("nope", vec![]));
    assert!(matches!(
        catalog.declare_predicate(id(2)?, vector).map_err(|e| e.0),
        Err(CatalogErrorEnum::UnknownVectorIndex { .. })
    ));
    assert!(catalog.predicate(id(2)?).is_none());
    let hnsw =
        PredicateDeclaration::new(ex("p")).with_vector_index(VectorIndexSpec::new("hnsw", vec![]));
    catalog.declare_predicate(id(2)?, hnsw.clone())?;
    let exact = PredicateDeclaration::new(ex("p")).with_tokenizers(vec!["exact".into()]);
    catalog.declare(id(2)?, Declaration::Predicate(exact.clone()), None)?;
    assert_eq!(catalog.predicate(id(2)?), Some(&exact));
    assert!(catalog.remove_predicate(id(2)?) && !catalog.remove_predicate(id(2)?));
    catalog.declare(
        id(3)?,
        Declaration::Relation(RelationDeclaration::new(ex("R"), vec![ex("role")])),
        None,
    )?;
    catalog.declare(
        id(4)?,
        Declaration::EntityType(TypeDeclaration::new(ex("T"), None, true)),
        None,
    )?;
    assert!(
        catalog.relation(id(3)?).is_some()
            && catalog.entity_type(id(4)?).context("type 4")?.is_abstract()
    );
    Ok(())
}
