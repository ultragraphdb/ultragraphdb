/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The steps of schema storage over a changeset applier standing in for a partition: catalog versions committed to a
//! test metadata group, taking effect in the partition as rounds of their own and loaded again after a restart, and
//! the default entries its rounds ask the group to record. The group's own half, committing a version and finding a
//! record, is the double's here and `ultragraphdb_cluster_metadata`'s in a cluster.

use std::sync::Arc;

use cucumber::{then, when};
use ultragraphdb_core_catalog::{Catalog, CatalogContents};
use ultragraphdb_core_mapping::{Declaration, PredicateDeclaration};
use ultragraphdb_traits_catalog::{CatalogSource, HeldVersion, TestCatalogSource};

use crate::Steps;
use crate::changeset_steps::cs;
use crate::utils::changesets::{
    TENANT, TestChangesets, changeset, changesets_over, declaration, ex, holds_catalog_key, insert,
    uid, write,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// Sends every default entry the partition's rounds asked for to the group, as its host does once its rounds apply.
pub(crate) fn send_defaults(w: &mut Steps, a: &TestChangesets) -> TestResult {
    for entry in a.take_default_entries() {
        let _created: bool = w.source.record_default(&entry)?;
    }
    Ok(())
}

/// Commits version `version` of `tenant` declaring `declarations` to `source`, in effect from epoch `version`, and
/// makes it take effect in the partition `a` as the round installing it: the refusals it met.
pub(crate) fn take_effect(
    source: &TestCatalogSource,
    a: &TestChangesets,
    tenant: u64,
    version: u64,
    declarations: Vec<Declaration>,
) -> TestResult<Vec<String>> {
    let contents = CatalogContents::new(tenant, version, declarations);
    source.commit(HeldVersion::new(
        tenant,
        version,
        version,
        contents.encode()?,
    ));
    let execution = a.execute_catalog(&contents)?;
    let refusals = execution
        .refusals()
        .iter()
        .map(ToString::to_string)
        .collect();
    let next = a.quads().lists().next_version();
    let _applied = a.apply_round(vec![(next, execution.into_executed())])?;
    Ok(refusals)
}

/// The partition over `a`'s store after a crash and a restart, which loads every tenant of `tenants` from the
/// version `source` holds in effect, decoded as every role loads it.
pub(crate) fn restarted(
    source: &TestCatalogSource,
    a: &TestChangesets,
    tenants: &[u64],
) -> TestResult<TestChangesets> {
    let store = a.quads().store().clone();
    store.crash();
    store.recover();
    let b = changesets_over(store)?;
    for &tenant in tenants {
        if let Some(held) = source.version_at(tenant, u64::MAX)? {
            let contents = CatalogContents::decode(tenant, held.version(), held.contents())?;
            b.load_catalog(&contents)?;
        }
    }
    b.recover_builds()?;
    Ok(b)
}

/// The workload of the restart scenarios: `name` a property with a `term` index and `knows` an edge.
fn declared() -> TestResult<Vec<Declaration>> {
    Ok(vec![
        Declaration::Predicate(declaration("name", "a property with `term`")?),
        Declaration::Predicate(declaration("knows", "an edge")?),
    ])
}

#[when(regex = r#"^`<a> <nickname> "Al"` is written and `nickname` is not declared$"#)]
fn nickname_written(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    write(&a, &[r#"<a> <nickname> "Al""#])?;
    send_defaults(w, &a)
}

#[then(
    "once the command is answered the metadata group holds the default-entry record of `nickname`, and the catalog reads `nickname` with the default entry"
)]
fn nickname_recorded(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let default = PredicateDeclaration::new(ex("nickname"));
    let recorded: Vec<String> = w
        .source
        .defaults(TENANT)
        .iter()
        .map(|e| e.predicate().to_string())
        .collect();
    assert!(
        recorded.contains(&ex("nickname").as_str().to_string()),
        "{recorded:?}"
    );
    let entry = w
        .source
        .defaults(TENANT)
        .into_iter()
        .find(|e| e.predicate() == ex("nickname").as_str())
        .context("the record of `nickname`")?;
    assert_eq!(
        ultragraphdb_core_catalog::decode_declaration(&[], entry.entry())?,
        Declaration::Predicate(default.clone())
    );
    let catalog = a.catalog(TENANT)?;
    assert_eq!(catalog.predicate(uid(&a, "nickname")?), Some(&default));
    Ok(())
}

#[when("a write naming `<alias>` fails before it commits and `alias` was not declared")]
fn alias_refused(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let _refusals = take_effect(
        &w.source,
        &a,
        TENANT,
        2,
        vec![Declaration::Predicate(declaration("knows", "an edge")?)],
    )?;
    let refused = a.apply(&changeset(vec![
        insert(r#"<a> <alias> "Al""#)?,
        insert(r#"<a> <knows> "Bob""#)?,
    ]));
    assert_eq!(
        refused
            .err()
            .context("a literal on the edge `knows` is refused")?
            .to_string(),
        "Predicate <http://ex.org/knows> is declared an edge and cannot hold the literal \"Bob\""
    );
    send_defaults(w, &a)
}

#[when(
    regex = r#"^two writes read before either commits, the first writes `(.+)` and the second `(.+)`, and `score` was not declared$"#
)]
fn concurrent_first_writes(w: &mut Steps, first: String, second: String) -> TestResult {
    let a = cs(w)?;
    let executed: Vec<_> = [first, second]
        .iter()
        .map(|text| Ok(a.execute(&changeset(vec![insert(text)?]))?))
        .collect::<TestResult<_>>()?;
    let next = a.quads().lists().next_version();
    let round = executed
        .into_iter()
        .zip([next, next + 2])
        .map(|(e, v)| (v, e))
        .collect();
    let applied = a.apply_round(round).context("the round is written")?;
    w.results.extend(applied.into_iter().map(Ok));
    send_defaults(w, &a)
}

#[then("both commit, and the catalog holds `score` with the default entry")]
fn both_commit_with_default(w: &mut Steps) -> TestResult {
    assert_eq!(w.results.len(), 2);
    assert!(w.results.iter().all(Result::is_ok));
    let a = cs(w)?;
    assert_eq!(
        a.catalog(TENANT)?.predicate(uid(&a, "score")?),
        Some(&PredicateDeclaration::new(ex("score")))
    );
    let recorded: Vec<String> = w
        .source
        .defaults(TENANT)
        .iter()
        .map(|e| e.predicate().to_string())
        .filter(|p| p == ex("score").as_str())
        .collect();
    assert_eq!(
        recorded.len(),
        1,
        "the group holds `score`'s default entry once"
    );
    Ok(())
}

#[when("declarations are made and the frontend and storage are stopped and started")]
fn declared_and_restarted(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let _refusals = take_effect(&w.source, &a, TENANT, 2, declared()?)?;
    write(&a, &[r#"<a> <name> "big apple""#, "<a> <knows> <b>"])?;
    w.catalogs = vec![(*a.catalog(TENANT)?).clone()];
    let b = restarted(&w.source, &a, &[TENANT])?;
    w.changesets = Some(Arc::new(b));
    Ok(())
}

#[then("the loaded catalog equals the catalog before stopping")]
fn loaded_equals(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let before: &Catalog = w.catalogs.first().context("the catalog before stopping")?;
    assert_eq!(*a.catalog(TENANT)?, *before);
    assert!(!holds_catalog_key(&a));
    Ok(())
}

#[when("a predicate's declaration is removed and the roles are restarted")]
fn removed_and_restarted(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let _refusals = take_effect(&w.source, &a, TENANT, 2, declared()?)?;
    write(&a, &[r#"<a> <name> "big apple""#])?;
    let kept = vec![Declaration::Predicate(declaration("knows", "an edge")?)];
    let removed = take_effect(&w.source, &a, TENANT, 3, kept)?;
    assert!(removed.is_empty(), "{removed:?}");
    let b = restarted(&w.source, &a, &[TENANT])?;
    w.changesets = Some(Arc::new(b));
    Ok(())
}

#[then("the loaded catalog does not declare the predicate")]
fn removed_stays_removed(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let catalog = a.catalog(TENANT)?;
    assert!(catalog.predicate(uid(&a, "name")?).is_none());
    assert!(catalog.predicate(uid(&a, "knows")?).is_some());
    assert_eq!(catalog.version(), 3);
    Ok(())
}

#[when("a predicate is declared")]
fn predicate_declared(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let _refusals = take_effect(&w.source, &a, TENANT, 2, declared()?)?;
    write(&a, &[r#"<a> <name> "pear""#])?;
    Ok(())
}

#[then("no partition's store holds a catalog key for it")]
fn no_catalog_key(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    assert!(a.catalog(TENANT)?.predicate(uid(&a, "name")?).is_some());
    assert!(!holds_catalog_key(&a));
    Ok(())
}
