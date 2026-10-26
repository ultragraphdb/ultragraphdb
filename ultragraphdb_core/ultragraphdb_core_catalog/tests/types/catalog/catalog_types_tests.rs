/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;
use ultragraphdb_core_catalog::{Catalog, CatalogErrorEnum};
use ultragraphdb_core_mapping::TypeDeclaration;

fn ex(name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

fn declare(
    catalog: &mut Catalog,
    uid: u64,
    name: &str,
    parent: Option<(&str, u64)>,
) -> Result<(), ultragraphdb_core_catalog::CatalogError> {
    catalog.declare_type(
        uid,
        TypeDeclaration::new(ex(name), parent.map(|(p, _)| ex(p)), false),
        parent.map(|(_, u)| u),
    )
}

#[test]
fn test_a_cycle_is_refused_and_redeclaring_the_same_parent_is_not() {
    let mut c = Catalog::new(0);
    declare(&mut c, 1, "A", None).unwrap();
    declare(&mut c, 2, "B", Some(("A", 1))).unwrap();
    declare(&mut c, 3, "C", Some(("B", 2))).unwrap();
    declare(&mut c, 2, "B", Some(("A", 1))).unwrap();
    let error = declare(&mut c, 1, "A", Some(("C", 3))).unwrap_err();
    assert!(
        matches!(error.0, CatalogErrorEnum::ParentCycle { .. }),
        "{error}"
    );
    let error = declare(&mut c, 4, "D", Some(("D", 4))).unwrap_err();
    assert!(matches!(error.0, CatalogErrorEnum::ParentCycle { .. }));
    assert!(c.entity_type(1).unwrap().parent().is_none());
}

#[test]
fn test_preorder_numbers_cover_every_type_with_descendants_after_their_ancestor() {
    let mut c = Catalog::new(0);
    declare(&mut c, 5, "Root2", None).unwrap();
    declare(&mut c, 1, "Root1", None).unwrap();
    declare(&mut c, 7, "Leaf", Some(("Mid", 6))).unwrap_or(());
    declare(&mut c, 6, "Mid", Some(("Root1", 1))).unwrap();
    declare(&mut c, 7, "Leaf", Some(("Mid", 6))).unwrap();
    let n = c.preorder();
    assert_eq!(n.len(), 4);
    assert_eq!((n[&1], n[&6], n[&7], n[&5]), (1, 2, 3, 4));
    let mut numbers: Vec<u32> = n.values().copied().collect();
    numbers.sort();
    assert_eq!(numbers, vec![1, 2, 3, 4]);
}
