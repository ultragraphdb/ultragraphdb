/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;
use ultragraphdb_core_catalog::{Catalog, CatalogErrorEnum};
use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_mapping::TypeDeclaration;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

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

fn declare(
    catalog: &mut Catalog,
    uid: u64,
    name: &str,
    parent: Option<(&str, u64)>,
) -> TestResult<Result<(), ultragraphdb_core_catalog::CatalogError>> {
    let parent_id = parent.map(|(_, u)| id(u)).transpose()?;
    Ok(catalog.declare_type(
        id(uid)?,
        TypeDeclaration::new(ex(name), parent.map(|(p, _)| ex(p)), false),
        parent_id,
    ))
}

#[test]
fn test_a_cycle_is_refused_and_redeclaring_the_same_parent_is_not() -> TestResult {
    let mut c = Catalog::new(0);
    declare(&mut c, 1, "A", None)??;
    declare(&mut c, 2, "B", Some(("A", 1)))??;
    declare(&mut c, 3, "C", Some(("B", 2)))??;
    declare(&mut c, 2, "B", Some(("A", 1)))??;
    let Err(error) = declare(&mut c, 1, "A", Some(("C", 3)))? else {
        return Err(TestError::new(
            "declare returned Ok where an error was expected",
        ));
    };
    assert!(
        matches!(error.0, CatalogErrorEnum::ParentCycle { .. }),
        "{error}"
    );
    let Err(own_parent) = declare(&mut c, 4, "D", Some(("D", 4)))? else {
        return Err(TestError::new(
            "declare returned Ok where an error was expected",
        ));
    };
    assert!(matches!(own_parent.0, CatalogErrorEnum::ParentCycle { .. }));
    assert!(c.entity_type(id(1)?).context("type 1")?.parent().is_none());
    Ok(())
}

#[test]
fn test_preorder_numbers_cover_every_type_with_descendants_after_their_ancestor() -> TestResult {
    let mut c = Catalog::new(0);
    declare(&mut c, 5, "Root2", None)??;
    declare(&mut c, 1, "Root1", None)??;
    declare(&mut c, 7, "Leaf", Some(("Mid", 6)))?.unwrap_or(());
    declare(&mut c, 6, "Mid", Some(("Root1", 1)))??;
    declare(&mut c, 7, "Leaf", Some(("Mid", 6)))??;
    let n = c.preorder();
    assert_eq!(n.len(), 4);
    assert_eq!(
        (
            n.get(&id(1)?),
            n.get(&id(6)?),
            n.get(&id(7)?),
            n.get(&id(5)?)
        ),
        (Some(&1), Some(&2), Some(&3), Some(&4))
    );
    let mut numbers: Vec<u32> = n.values().copied().collect();
    numbers.sort();
    assert_eq!(numbers, vec![1, 2, 3, 4]);
    Ok(())
}
