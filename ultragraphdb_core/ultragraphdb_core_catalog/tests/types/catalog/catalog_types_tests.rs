/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;
use ultragraphdb_core_catalog::{Catalog, CatalogErrorEnum};
use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_mapping::{Declaration, TypeDeclaration};
use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimStore};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_traits_store::{PersistMode, Store, WriteBatch};

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
    assert_eq!(
        error.0,
        CatalogErrorEnum::ParentCycle {
            entity_type: "http://ex.org/A".to_string(),
            parent: "http://ex.org/C".to_string(),
        }
    );
    let Err(own_parent) = declare(&mut c, 4, "D", Some(("D", 4)))? else {
        return Err(TestError::new(
            "declare returned Ok where an error was expected",
        ));
    };
    assert_eq!(
        own_parent.0,
        CatalogErrorEnum::ParentCycle {
            entity_type: "http://ex.org/D".to_string(),
            parent: "http://ex.org/D".to_string(),
        }
    );
    assert_eq!(c.entity_type(id(4)?), None);
    assert!(c.entity_type(id(1)?).context("type 1")?.parent().is_none());
    Ok(())
}

#[test]
fn test_preorder_numbers_cover_every_type_with_descendants_after_their_ancestor() -> TestResult {
    let mut c = Catalog::new(0);
    declare(&mut c, 5, "Root2", None)??;
    declare(&mut c, 1, "Root1", None)??;
    // A parent not yet declared is taken: the walk up the hierarchy stops at it.
    declare(&mut c, 7, "Leaf", Some(("Mid", 6)))??;
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

#[test]
fn test_a_second_parent_is_refused_naming_both() -> TestResult {
    let mut c = Catalog::new(0);
    declare(&mut c, 1, "Person", None)??;
    declare(&mut c, 2, "Agent", None)??;
    declare(&mut c, 3, "Employee", Some(("Person", 1)))??;
    let refused = declare(&mut c, 3, "Employee", Some(("Agent", 2)))?
        .err()
        .context("a refusal")?;
    assert_eq!(
        refused.0,
        CatalogErrorEnum::SecondParent {
            entity_type: "http://ex.org/Employee".to_string(),
            parent: "http://ex.org/Person".to_string(),
            other: "http://ex.org/Agent".to_string(),
        }
    );
    let held = c.entity_type(id(3)?).context("type 3")?;
    assert_eq!(held.parent(), Some(&ex("Person")));
    Ok(())
}

#[test]
fn test_a_cycle_closed_through_a_parent_not_yet_declared_is_refused() -> TestResult {
    // A names B as its parent before B is declared; B then naming A would make each the other's ancestor.
    let mut c = Catalog::new(0);
    declare(&mut c, 1, "A", Some(("B", 2)))??;
    let refused = declare(&mut c, 2, "B", Some(("A", 1)))?
        .err()
        .context("a refusal")?;
    assert_eq!(
        refused.0,
        CatalogErrorEnum::ParentCycle {
            entity_type: "http://ex.org/B".to_string(),
            parent: "http://ex.org/A".to_string(),
        }
    );
    assert_eq!(c.entity_type(id(2)?), None);
    // Longer: C under B under A, A's parent C named first.
    let mut longer = Catalog::new(0);
    declare(&mut longer, 1, "A", Some(("C", 3)))??;
    declare(&mut longer, 2, "B", Some(("A", 1)))??;
    let refused_longer = declare(&mut longer, 3, "C", Some(("B", 2)))?
        .err()
        .context("a refusal")?;
    assert_eq!(
        refused_longer.0,
        CatalogErrorEnum::ParentCycle {
            entity_type: "http://ex.org/C".to_string(),
            parent: "http://ex.org/B".to_string(),
        }
    );
    Ok(())
}

/// A catalog of tenant 0 loaded from a store holding `types`, each `(uid, name, parent)`, written as entries.
fn loaded(types: &[(u64, &str, Option<&str>)]) -> TestResult<Catalog> {
    let store = SimStore::new(Seed::new(3), FaultConfig::none(), SimClock::new());
    let mut batch = WriteBatch::new();
    for (uid, name, parent) in types {
        let declaration =
            Declaration::EntityType(TypeDeclaration::new(ex(name), parent.map(ex), false));
        batch.push(Catalog::entry(0, id(*uid)?, &declaration, 1)?);
    }
    store.apply(batch, PersistMode::Buffer)?;
    Ok(Catalog::load(&store.snapshot(u64::MAX)?, 0)?)
}

#[test]
fn test_a_stored_cycle_refuses_a_type_declared_below_it() -> TestResult {
    // Stored entries can hold a cycle no declaration would make: A and B each the other's parent.
    let mut c = loaded(&[(1, "A", Some("B")), (2, "B", Some("A")), (4, "Root", None)])?;
    let refused = declare(&mut c, 3, "C", Some(("A", 1)))?
        .err()
        .context("a refusal")?;
    assert_eq!(
        refused.0,
        CatalogErrorEnum::ParentCycle {
            entity_type: "http://ex.org/C".to_string(),
            parent: "http://ex.org/A".to_string(),
        }
    );
    assert_eq!(c.entity_type(id(3)?), None);
    // The types on the cycle are reached from no root and left unnumbered.
    let numbers = c.preorder();
    assert_eq!(numbers.into_iter().collect::<Vec<_>>(), vec![(id(4)?, 1)]);
    Ok(())
}

#[test]
fn test_an_empty_catalog_numbers_nothing() {
    assert!(Catalog::new(0).preorder().is_empty());
}

#[test]
fn test_redeclaring_a_type_without_its_parent_is_refused() -> TestResult {
    let mut c = Catalog::new(0);
    declare(&mut c, 1, "Person", None)??;
    declare(&mut c, 3, "Employee", Some(("Person", 1)))??;
    let before = c.clone();
    let refused = declare(&mut c, 3, "Employee", None)?
        .err()
        .context("a refusal")?;
    assert_eq!(
        refused.0,
        CatalogErrorEnum::ParentDropped {
            entity_type: "http://ex.org/Employee".to_string(),
            parent: "http://ex.org/Person".to_string(),
        }
    );
    assert_eq!(
        refused.to_string(),
        "Type <http://ex.org/Employee> has the parent <http://ex.org/Person> and cannot be declared without it"
    );
    assert_eq!(c, before, "nothing changes on the refusal");
    // The same parent again is taken, and a root stays a root.
    declare(&mut c, 3, "Employee", Some(("Person", 1)))??;
    declare(&mut c, 1, "Person", None)??;
    assert_eq!(c, before);
    Ok(())
}

#[test]
fn test_a_declaration_dropping_a_parent_is_refused_through_declare() -> TestResult {
    let mut c = Catalog::new(0);
    declare(&mut c, 1, "Person", None)??;
    declare(&mut c, 3, "Employee", Some(("Person", 1)))??;
    let refused = c
        .declare(
            id(3)?,
            Declaration::EntityType(TypeDeclaration::new(ex("Employee"), None, false)),
            None,
        )
        .err()
        .context("a refusal")?;
    assert_eq!(
        refused.0,
        CatalogErrorEnum::ParentDropped {
            entity_type: "http://ex.org/Employee".to_string(),
            parent: "http://ex.org/Person".to_string(),
        }
    );
    // Removing the type first, then declaring it a root, is the way to drop its parent.
    assert!(c.remove_entity_type(id(3)?));
    declare(&mut c, 3, "Employee", None)??;
    assert_eq!(
        c.entity_type(id(3)?).and_then(TypeDeclaration::parent),
        None
    );
    Ok(())
}
