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
    assert_eq!(catalog.tenant(), 4);
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
    assert_eq!(
        catalog.declare_predicate(id(2)?, unknown).map_err(|e| e.0),
        Err(CatalogErrorEnum::UnknownTokenizer {
            predicate: "http://ex.org/p".to_string(),
            tokenizer: "nope".to_string(),
        })
    );
    let vector =
        PredicateDeclaration::new(ex("p")).with_vector_index(VectorIndexSpec::new("nope", vec![]));
    assert_eq!(
        catalog.declare_predicate(id(2)?, vector).map_err(|e| e.0),
        Err(CatalogErrorEnum::UnknownVectorIndex {
            predicate: "http://ex.org/p".to_string(),
            factory: "nope".to_string(),
        })
    );
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

#[test]
fn test_a_declaration_with_more_tokenizers_than_the_encoding_holds_is_refused() -> TestResult {
    let many: Vec<String> = std::iter::repeat_n("exact".to_string(), 65_536).collect();
    let declaration =
        PredicateDeclaration::new(oxrdf::NamedNode::new_unchecked("http://ex.org/many"))
            .with_tokenizers(many);
    let refused = Catalog::check_predicate(&declaration).map_err(|e| e.0);
    assert_eq!(
        refused,
        Err(CatalogErrorEnum::TooLarge {
            iri: "http://ex.org/many".to_string(),
            what: "tokenizers",
            len: 65_536,
            max: 65_535,
        })
    );
    let fits: Vec<String> = std::iter::repeat_n("exact".to_string(), 65_535).collect();
    let fitting = PredicateDeclaration::new(oxrdf::NamedNode::new_unchecked("http://ex.org/many"))
        .with_tokenizers(fits);
    assert_eq!(Catalog::check_predicate(&fitting), Ok(()));
    Ok(())
}

#[test]
fn test_a_tokenizer_taking_no_value_of_the_constraint_is_refused() -> TestResult {
    let mut catalog = Catalog::new(0);
    let age = PredicateDeclaration::new(ex("age"))
        .with_datatype(oxrdf::vocab::xsd::STRING.into_owned())
        .with_tokenizers(vec!["exact".into(), "int".into()]);
    assert_eq!(
        catalog.declare_predicate(id(2)?, age).map_err(|e| e.0),
        Err(CatalogErrorEnum::TokenizerDatatype {
            predicate: "http://ex.org/age".to_string(),
            tokenizer: "int".to_string(),
            datatype: oxrdf::vocab::xsd::STRING.as_str().to_string(),
        })
    );
    // An unknown tokenizer is refused before its datatype is looked at.
    let unknown = PredicateDeclaration::new(ex("age"))
        .with_datatype(oxrdf::vocab::xsd::STRING.into_owned())
        .with_tokenizers(vec!["nope".into()]);
    assert_eq!(
        Catalog::check_predicate(&unknown).map_err(|e| e.0),
        Err(CatalogErrorEnum::UnknownTokenizer {
            predicate: "http://ex.org/age".to_string(),
            tokenizer: "nope".to_string(),
        })
    );
    // No constraint: any known tokenizer is taken.
    let unconstrained = PredicateDeclaration::new(ex("age")).with_tokenizers(vec!["int".into()]);
    assert_eq!(Catalog::check_predicate(&unconstrained), Ok(()));
    assert!(catalog.predicate(id(2)?).is_none());
    Ok(())
}

#[test]
fn test_a_refused_declaration_leaves_the_one_held() -> TestResult {
    let mut catalog = Catalog::new(0);
    let exact = PredicateDeclaration::new(ex("p")).with_tokenizers(vec!["exact".into()]);
    catalog.declare_predicate(id(2)?, exact.clone())?;
    let unknown = PredicateDeclaration::new(ex("p")).with_tokenizers(vec!["nope".into()]);
    assert_eq!(
        catalog
            .declare(id(2)?, Declaration::Predicate(unknown), None)
            .map_err(|e| e.0),
        Err(CatalogErrorEnum::UnknownTokenizer {
            predicate: "http://ex.org/p".to_string(),
            tokenizer: "nope".to_string(),
        })
    );
    assert_eq!(catalog.predicate(id(2)?), Some(&exact));
    Ok(())
}

#[test]
fn test_a_declared_predicate_behaves_as_its_own_declaration() -> TestResult {
    let mut catalog = Catalog::new(0);
    let edge = PredicateDeclaration::new(ex("knows")).with_kind(PredicateKind::Edge);
    catalog.declare_predicate(id(5)?, edge.clone())?;
    let declared = catalog.predicate_or_default(id(5)?, &ex("ignored"));
    assert!(matches!(declared, std::borrow::Cow::Borrowed(_)));
    assert_eq!(*declared, edge);
    let listed: Vec<_> = catalog.predicates().collect();
    assert_eq!(listed, vec![(id(5)?, &edge)]);
    Ok(())
}

#[test]
fn test_types_and_relations_are_listed_and_removed() -> TestResult {
    let mut catalog = Catalog::new(0);
    let relation = RelationDeclaration::new(ex("R"), vec![ex("a"), ex("b")]);
    let entity = TypeDeclaration::new(ex("T"), None, false);
    catalog.declare(id(3)?, Declaration::Relation(relation.clone()), None)?;
    catalog.declare(id(4)?, Declaration::EntityType(entity.clone()), None)?;
    assert_eq!(
        catalog.relations().collect::<Vec<_>>(),
        vec![(id(3)?, &relation)]
    );
    assert_eq!(
        catalog.entity_types().collect::<Vec<_>>(),
        vec![(id(4)?, &entity)]
    );
    assert!(catalog.remove_relation(id(3)?));
    assert!(!catalog.remove_relation(id(3)?));
    assert!(catalog.remove_entity_type(id(4)?));
    assert!(!catalog.remove_entity_type(id(4)?));
    assert_eq!(catalog.relation(id(3)?), None);
    assert_eq!(catalog.entity_type(id(4)?), None);
    assert_eq!(catalog, Catalog::new(0));
    Ok(())
}

#[test]
fn test_a_relation_with_more_roles_than_the_encoding_holds_is_refused() -> TestResult {
    // 65,536 roles: one past the encoding's u16 count, which `Catalog::entry` would wrap to 0 and a load then refuse.
    let roles: Vec<NamedNode> = (0..=u16::MAX).map(|i| ex(&i.to_string())).collect();
    let mut catalog = Catalog::new(0);
    let refused = catalog
        .declare(
            id(3)?,
            Declaration::Relation(RelationDeclaration::new(ex("R"), roles)),
            None,
        )
        .map_err(|e| e.0);
    assert_eq!(
        refused,
        Err(CatalogErrorEnum::TooLarge {
            iri: "http://ex.org/R".to_string(),
            what: "roles",
            len: 65_536,
            max: 65_535,
        })
    );
    assert_eq!(catalog.relation(id(3)?), None);
    let fits: Vec<NamedNode> = (0..u16::MAX).map(|i| ex(&i.to_string())).collect();
    catalog.declare(
        id(3)?,
        Declaration::Relation(RelationDeclaration::new(ex("R"), fits)),
        None,
    )?;
    assert_eq!(
        catalog.relation(id(3)?).map(|r| r.roles().len()),
        Some(65_535)
    );
    Ok(())
}

#[test]
fn test_a_relation_declared_directly_with_more_roles_than_the_encoding_holds_is_refused()
-> TestResult {
    let mut catalog = Catalog::new(0);
    let held = RelationDeclaration::new(ex("R"), vec![ex("a")]);
    catalog.declare_relation(id(3)?, held.clone())?;
    let roles: Vec<NamedNode> = (0..=u16::MAX).map(|i| ex(&i.to_string())).collect();
    let refused = catalog
        .declare_relation(id(3)?, RelationDeclaration::new(ex("R"), roles))
        .map_err(|e| e.0);
    assert_eq!(
        refused,
        Err(CatalogErrorEnum::TooLarge {
            iri: "http://ex.org/R".to_string(),
            what: "roles",
            len: 65_536,
            max: 65_535,
        })
    );
    assert_eq!(
        catalog.relation(id(3)?),
        Some(&held),
        "nothing changes on the refusal"
    );
    let fits: Vec<NamedNode> = (0..u16::MAX).map(|i| ex(&i.to_string())).collect();
    catalog.declare_relation(id(3)?, RelationDeclaration::new(ex("R"), fits))?;
    assert_eq!(
        catalog.relation(id(3)?).map(|r| r.roles().len()),
        Some(65_535)
    );
    Ok(())
}

#[test]
fn test_a_declaration_held_by_name_governs_until_a_new_version_says_it_again() -> TestResult {
    let iri = NamedNode::new("http://ex.org/name")?;
    let mut catalog = Catalog::new(1);
    let declared = PredicateDeclaration::new(iri.clone()).with_kind(PredicateKind::Property);
    catalog.hold_unresolved(declared.clone())?;
    assert_eq!(catalog.unresolved(iri.as_str()), Some(&declared));
    let uid = id(1)?;
    assert_eq!(catalog.predicate(uid), None, "no ID resolves it here");
    assert_eq!(catalog.predicate_or_default(uid, &iri).as_ref(), &declared);
    let given = catalog.with_initial(id(2)?);
    assert_eq!(given.tenant(), 1);
    assert_eq!(given.unresolved(iri.as_str()), Some(&declared));
    assert!(given.predicate(id(2)?).is_some());
    let mut emptied = given.emptied();
    assert_eq!(emptied.predicates().count(), 0);
    assert_eq!(emptied.unresolved(iri.as_str()), Some(&declared));
    emptied.clear_unresolved();
    assert_eq!(emptied.unresolved(iri.as_str()), None);
    assert_eq!(
        emptied.predicate_or_default(uid, &iri).as_ref(),
        &PredicateDeclaration::new(iri)
    );
    Ok(())
}

/// An emptied catalog declares nothing by ID and keeps its tenant and its version.
#[test]
fn test_an_emptied_catalog_keeps_its_tenant_and_its_version() -> TestResult {
    let emptied = Catalog::new(3)
        .with_version(7)
        .with_initial(id(2)?)
        .emptied();
    assert_eq!((emptied.tenant(), emptied.version()), (3, 7));
    assert_eq!(emptied.predicates().count(), 0);
    Ok(())
}
