/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::vocab::xsd;
use oxrdf::{BlankNode, Literal, NamedNode, Term, Triple};
use ultragraphdb_core_catalog::{Catalog, CatalogErrorEnum, datatype_of};
use ultragraphdb_core_mapping::{PredicateDeclaration, PredicateKind};
use ultragraphdb_tool_bdd::{Context, TestResult};

fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

/// The refusal of `object` for `declaration`, or why there is none.
fn refusal(declaration: &PredicateDeclaration, object: &Term) -> TestResult<CatalogErrorEnum> {
    Ok(Catalog::check_object(declaration, object.as_ref())
        .err()
        .context("a refusal")?
        .0)
}

#[test]
fn test_an_object_is_checked_against_the_kind_and_the_datatype_constraint() -> TestResult {
    let edge = PredicateDeclaration::new(ex("knows")).with_kind(PredicateKind::Edge);
    let property = PredicateDeclaration::new(ex("name")).with_kind(PredicateKind::Property);
    let typed = PredicateDeclaration::new(ex("age")).with_datatype(xsd::INTEGER.into_owned());
    let bob = Term::from(Literal::new_simple_literal("Bob"));
    let b = Term::from(ex("b"));
    let blank = Term::from(BlankNode::new_unchecked("x"));
    assert_eq!(
        refusal(&edge, &bob)?,
        CatalogErrorEnum::LiteralOnEdge {
            predicate: "http://ex.org/knows".to_string(),
            object: "\"Bob\"".to_string(),
        }
    );
    assert_eq!(Catalog::check_object(&edge, b.as_ref()), Ok(()));
    assert_eq!(Catalog::check_object(&edge, blank.as_ref()), Ok(()));
    assert_eq!(
        refusal(&property, &blank)?,
        CatalogErrorEnum::ResourceOnProperty {
            predicate: "http://ex.org/name".to_string(),
            object: "_:x".to_string(),
        }
    );
    assert_eq!(
        refusal(&property, &b)?,
        CatalogErrorEnum::ResourceOnProperty {
            predicate: "http://ex.org/name".to_string(),
            object: "<http://ex.org/b>".to_string(),
        }
    );
    assert_eq!(Catalog::check_object(&property, bob.as_ref()), Ok(()));
    let old = Term::from(Literal::new_simple_literal("old"));
    assert_eq!(
        refusal(&typed, &old)?,
        CatalogErrorEnum::DatatypeMismatch {
            predicate: "http://ex.org/age".to_string(),
            expected: xsd::INTEGER.as_str().to_string(),
            found: xsd::STRING.as_str().to_string(),
        }
    );
    assert_eq!(
        Catalog::check_object(&typed, Term::from(Literal::from(3)).as_ref()),
        Ok(())
    );
    assert_eq!(
        Catalog::check_object(&typed, b.as_ref()),
        Ok(()),
        "the constraint holds literals only"
    );
    assert_eq!(datatype_of(bob.as_ref()), Some(xsd::STRING.as_str()));
    assert_eq!(datatype_of(b.as_ref()), None);
    Ok(())
}

#[test]
fn test_a_predicate_of_either_kind_takes_every_object_its_constraint_allows() -> TestResult {
    let either = PredicateDeclaration::new(ex("any"));
    let typed_either =
        PredicateDeclaration::new(ex("count")).with_datatype(xsd::INTEGER.into_owned());
    let triple = Term::from(Triple::new(ex("s"), ex("p"), ex("o")));
    for object in [
        Term::from(Literal::new_simple_literal("x")),
        Term::from(ex("b")),
        Term::from(BlankNode::new_unchecked("y")),
        triple.clone(),
    ] {
        assert_eq!(
            Catalog::check_object(&either, object.as_ref()),
            Ok(()),
            "{object}"
        );
    }
    let tagged = Term::from(Literal::new_language_tagged_literal_unchecked("x", "en"));
    assert_eq!(
        refusal(&typed_either, &tagged)?,
        CatalogErrorEnum::DatatypeMismatch {
            predicate: "http://ex.org/count".to_string(),
            expected: xsd::INTEGER.as_str().to_string(),
            found: "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString".to_string(),
        }
    );
    // A quoted triple is a resource: an edge takes it, a property does not.
    let edge = PredicateDeclaration::new(ex("knows")).with_kind(PredicateKind::Edge);
    assert_eq!(Catalog::check_object(&edge, triple.as_ref()), Ok(()));
    let property = PredicateDeclaration::new(ex("name")).with_kind(PredicateKind::Property);
    assert_eq!(
        refusal(&property, &triple)?,
        CatalogErrorEnum::ResourceOnProperty {
            predicate: "http://ex.org/name".to_string(),
            object: "<<( <http://ex.org/s> <http://ex.org/p> <http://ex.org/o> )>>".to_string(),
        }
    );
    Ok(())
}

#[test]
fn test_only_a_literal_has_a_datatype() {
    assert_eq!(
        datatype_of(Term::from(BlankNode::new_unchecked("x")).as_ref()),
        None
    );
    assert_eq!(
        datatype_of(Term::from(Triple::new(ex("s"), ex("p"), ex("o"))).as_ref()),
        None
    );
    assert_eq!(
        datatype_of(Term::from(Literal::from(3)).as_ref()),
        Some(xsd::INTEGER.as_str())
    );
}
