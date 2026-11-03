/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;
use ultragraphdb_core_catalog::{Catalog, CatalogContents};
use ultragraphdb_core_mapping::{Declaration, PredicateDeclaration, PredicateKind};
use ultragraphdb_tool_bdd::TestResult;

fn knows() -> Declaration {
    Declaration::Predicate(
        PredicateDeclaration::new(NamedNode::new_unchecked("http://ex.org/knows"))
            .with_kind(PredicateKind::Edge),
    )
}

#[test]
fn test_contents_read_back_what_they_were_made_of() -> TestResult {
    let contents = CatalogContents::new(3, 7, vec![knows()]);
    assert_eq!(contents.namespace(), 3);
    assert_eq!(contents.version(), 7);
    assert_eq!(contents.declarations(), &[knows()]);
    Ok(())
}

#[test]
fn test_empty_contents_declare_nothing() -> TestResult {
    let contents = CatalogContents::new(0, 1, Vec::new());
    assert!(contents.declarations().is_empty());
    assert_eq!(contents, CatalogContents::new(0, 1, Vec::new()));
    assert_ne!(contents, CatalogContents::new(0, 2, Vec::new()));
    assert_ne!(contents, CatalogContents::new(1, 1, Vec::new()));
    Ok(())
}

#[test]
fn test_a_catalog_carries_the_version_it_holds() -> TestResult {
    let catalog = Catalog::new(2);
    assert_eq!(catalog.version(), 0);
    let versioned = catalog.clone().with_version(9);
    assert_eq!(versioned.version(), 9);
    assert_eq!(versioned.namespace(), 2);
    assert_ne!(versioned, catalog, "the version is part of the catalog");
    assert_eq!(versioned.with_version(0), catalog);
    Ok(())
}
