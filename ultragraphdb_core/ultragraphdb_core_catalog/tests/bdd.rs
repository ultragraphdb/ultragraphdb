/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The catalog's scenarios: declarations refused, the type hierarchy's preorder numbers, and a catalog version that does
//! not decode refusing the load.

use cucumber::{World, then, when};
use ultragraphdb_core_catalog::{Catalog, CatalogContents, CatalogError, CatalogErrorEnum};
use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_mapping::{Declaration, PredicateDeclaration, TypeDeclaration};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_traits_catalog::{CatalogSource, HeldVersion, TestCatalogSource};

/// The ID of sequence `n` in bucket 0, of the built-in `Resource` type.
fn id(n: u64) -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::RESOURCE,
        Bucket::new(0)?,
        Sequence::new(n)?,
    ))
}

#[derive(Debug, Default, World)]
struct Steps {
    catalog: Catalog,
    refusals: Vec<CatalogError>,
    loaded: Option<Result<CatalogContents, CatalogError>>,
}

fn xsd(local: &str) -> oxrdf::NamedNode {
    oxrdf::NamedNode::new_unchecked(format!("http://www.w3.org/2001/XMLSchema#{local}"))
}

fn ex(name: &str) -> oxrdf::NamedNode {
    oxrdf::NamedNode::new_unchecked(format!("http://ex.org/{name}"))
}

#[when("age is declared with datatype xsd:string and an int index")]
fn declare_age_int(w: &mut Steps) -> TestResult {
    let declaration = PredicateDeclaration::new(ex("age"))
        .with_datatype(xsd("string"))
        .with_tokenizers(vec!["int".to_string()]);
    w.refusals
        .extend(w.catalog.declare_predicate(id(5)?, declaration).err());
    Ok(())
}

#[then("the declaration fails with an error naming age, int and xsd:string")]
fn age_refused(w: &mut Steps) -> TestResult {
    let error = w.refusals.first().context("refused")?.to_string();
    assert!(
        error.contains("age") && error.contains("int") && error.contains("XMLSchema#string"),
        "{error}"
    );
    assert!(
        w.catalog.predicate(id(5)?).is_none(),
        "nothing was declared"
    );
    Ok(())
}

#[when("Employee is declared a subtype of Person and then of Agent")]
fn second_parent(w: &mut Steps) -> TestResult {
    let (employee, person, agent) = (id(1)?, id(2)?, id(3)?);
    for (uid, name) in [(person, "Person"), (agent, "Agent")] {
        w.catalog
            .declare_type(uid, TypeDeclaration::new(ex(name), None, false), None)?;
    }
    w.catalog.declare_type(
        employee,
        TypeDeclaration::new(ex("Employee"), Some(ex("Person")), false),
        Some(person),
    )?;
    w.refusals.extend(
        w.catalog
            .declare_type(
                employee,
                TypeDeclaration::new(ex("Employee"), Some(ex("Agent")), false),
                Some(agent),
            )
            .err(),
    );
    Ok(())
}

#[then("the second declaration fails with an error naming Employee, Person and Agent")]
fn second_parent_refused(w: &mut Steps) -> TestResult {
    let error = w.refusals.first().context("refused")?.to_string();
    for name in ["Employee", "Person", "Agent"] {
        assert!(error.contains(name), "{error}");
    }
    assert_eq!(
        w.catalog.entity_type(id(1)?).and_then(|t| t.parent()),
        Some(&ex("Person"))
    );
    Ok(())
}

#[when("Employee is declared a subtype of Person and then declared again with no parent")]
fn parent_dropped(w: &mut Steps) -> TestResult {
    let (employee, person) = (id(1)?, id(2)?);
    w.catalog.declare_type(
        person,
        TypeDeclaration::new(ex("Person"), None, false),
        None,
    )?;
    w.catalog.declare_type(
        employee,
        TypeDeclaration::new(ex("Employee"), Some(ex("Person")), false),
        Some(person),
    )?;
    w.refusals.extend(
        w.catalog
            .declare_type(
                employee,
                TypeDeclaration::new(ex("Employee"), None, false),
                None,
            )
            .err(),
    );
    Ok(())
}

#[then(
    "the second declaration fails with an error naming Employee and Person, and Employee keeps Person"
)]
fn parent_dropped_refused(w: &mut Steps) -> TestResult {
    let error = w.refusals.first().context("refused")?.to_string();
    for name in ["Employee", "Person"] {
        assert!(error.contains(name), "{error}");
    }
    assert_eq!(
        w.catalog.entity_type(id(1)?).and_then(|t| t.parent()),
        Some(&ex("Person"))
    );
    Ok(())
}

#[when("Agent has the subtypes Person and Org, and Person the subtype Employee")]
fn hierarchy(w: &mut Steps) -> TestResult {
    let declare = |steps: &mut Steps, uid: u64, name: &str, parent: Option<(&str, u64)>| {
        let declaration = TypeDeclaration::new(ex(name), parent.map(|(p, _)| ex(p)), false);
        let parent_id = parent.map(|(_, u)| id(u)).transpose()?;
        steps
            .catalog
            .declare_type(id(uid)?, declaration, parent_id)
            .map_err(TestError::from)
    };
    declare(w, 10, "Agent", None)?;
    declare(w, 20, "Person", Some(("Agent", 10)))?;
    declare(w, 30, "Org", Some(("Agent", 10)))?;
    declare(w, 40, "Employee", Some(("Person", 20)))?;
    Ok(())
}

#[then("the preorder numbers place Employee between Person and Org")]
fn preorder_placed(w: &mut Steps) -> TestResult {
    let n = w.catalog.preorder();
    let at = |uid: u64| -> TestResult<u32> { n.get(&id(uid)?).copied().context("a numbered type") };
    assert!(at(20)? < at(40)? && at(40)? < at(30)?, "{n:?}");
    assert_eq!(at(10)?, 1);
    Ok(())
}

#[when(
    "a namespace's catalog is loaded from a metadata group holding a version that does not decode"
)]
fn load_corrupted_version(w: &mut Steps) -> TestResult {
    let group = TestCatalogSource::new();
    let mut bytes = CatalogContents::new(
        3,
        2,
        vec![Declaration::Predicate(PredicateDeclaration::new(ex(
            "name",
        )))],
    )
    .encode()?;
    // The version's last byte lost: its declaration ends early.
    let _last: Option<u8> = bytes.pop();
    group.commit(HeldVersion::new(3, 2, 40, bytes));
    // Every role loads a namespace's catalog as the version in effect, decoded.
    let held = group.version_at(3, 40)?.context("a version is in effect")?;
    w.loaded = Some(CatalogContents::decode(3, held.version(), held.contents()));
    Ok(())
}

#[then("the load fails with a corruption error naming the version, and nothing panics")]
fn load_refused(w: &mut Steps) -> TestResult {
    let Err(error) = w.loaded.take().context("loaded")? else {
        return Err(TestError::new("the load is refused"));
    };
    assert!(
        matches!(
            &error.0,
            CatalogErrorEnum::CorruptedVersion {
                namespace: 3,
                version: 2,
                ..
            }
        ),
        "{error:?}"
    );
    assert!(error.to_string().contains("version 2"), "{error}");
    Ok(())
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
