/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::vocab::xsd;
use oxrdf::{BlankNode, Literal, NamedNode, Term};
use ultragraphdb_core_catalog::{Catalog, CatalogErrorEnum, datatype_of};
use ultragraphdb_core_mapping::{PredicateDeclaration, PredicateKind};

fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

#[test]
fn test_an_object_is_checked_against_the_kind_and_the_datatype_constraint() {
    let edge = PredicateDeclaration::new(ex("knows")).with_kind(PredicateKind::Edge);
    let property = PredicateDeclaration::new(ex("name")).with_kind(PredicateKind::Property);
    let typed = PredicateDeclaration::new(ex("age")).with_datatype(xsd::INTEGER.into_owned());
    let bob = Term::from(Literal::new_simple_literal("Bob"));
    let b = Term::from(ex("b"));
    let blank = Term::from(BlankNode::new_unchecked("x"));
    let error = Catalog::check_object(&edge, bob.as_ref()).unwrap_err();
    assert!(matches!(error.0, CatalogErrorEnum::LiteralOnEdge { .. }));
    assert!(error.to_string().contains("knows") && error.to_string().contains("Bob"));
    assert!(Catalog::check_object(&edge, b.as_ref()).is_ok());
    assert!(matches!(
        Catalog::check_object(&property, blank.as_ref())
            .unwrap_err()
            .0,
        CatalogErrorEnum::ResourceOnProperty { .. }
    ));
    assert!(Catalog::check_object(&property, bob.as_ref()).is_ok());
    let error = Catalog::check_object(
        &typed,
        Term::from(Literal::new_simple_literal("old")).as_ref(),
    )
    .unwrap_err();
    let text = error.to_string();
    assert!(
        text.contains("age")
            && text.contains("XMLSchema#integer")
            && text.contains("XMLSchema#string"),
        "{text}"
    );
    assert!(Catalog::check_object(&typed, Term::from(Literal::from(3)).as_ref()).is_ok());
    assert!(
        Catalog::check_object(&typed, b.as_ref()).is_ok(),
        "the constraint holds literals only"
    );
    assert_eq!(datatype_of(bob.as_ref()), Some(xsd::STRING.as_str()));
    assert_eq!(datatype_of(b.as_ref()), None);
}
