/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The steps of the catalog-change scenarios over a changeset applier: index builds, which run on its stepped
//! executor only when a step runs them, the declarations its data refuses, and drops.

use std::ops::ControlFlow;
use std::sync::{Arc, Mutex, PoisonError};

use cucumber::{then, when};
use oxrdf::GraphName;
use oxrdf::vocab::rdf;
use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{DEFAULT_GRAPH, Keyspace, QuadKey};
use ultragraphdb_core_mapping::{Cardinality, Change, Changeset};
use ultragraphdb_core_tok::get_tokenizer;
use ultragraphdb_store_apply::{Applied, ChangesetApplier, ChangesetErrorEnum};
use ultragraphdb_store_posting::{Lists, ListsConfig, PostingError};
use ultragraphdb_store_sim::{Fault, LogCapture, Seed, SimListCache, SteppedExecutor, Stream};
use ultragraphdb_traits_store::PersistMode;

use crate::Steps;
use crate::changeset_steps::{cs, is_held, last};
use crate::utils::changesets::{
    TENANT, TestChangesets, apply, declare, declared, ex, exact_token, finish_builds,
    holds_catalog_key, indexed, insert, stored_keys, term_token, uid, write,
};
use crate::utils::probe::Probe;
use ultragraphdb_tool_bdd::{Context, TestResult};

/// Whether the store holds a key of the value index of `predicate` under `tokenizer`.
fn has_index_keys(a: &TestChangesets, predicate: &str, tokenizer: &str) -> TestResult<bool> {
    let p = uid(a, predicate)?;
    let id = get_tokenizer(tokenizer)
        .context("a tokenizer")?
        .identifier();
    Ok(stored_keys(a).iter().any(|k| {
        matches!(
            QuadKey::parse(k),
            Ok((QuadKey::ValueIndex { predicate: indexed, tokenizer: by, .. }, _)) if indexed == p && by == id
        )
    }))
}

/// The subjects the `tokenizer` index of `predicate` lists under `text`, in the default graph: the `exact` token of a
/// string, or the `term` token of a lowercase word, written out independently of the tokenizer.
fn listed(
    a: &TestChangesets,
    predicate: &str,
    tokenizer: &str,
    text: &str,
) -> TestResult<Vec<SystemId>> {
    let token = match tokenizer {
        "exact" => Ok(exact_token(text)),
        "term" => Ok(term_token(text)),
        other => Err(format!("no written-out token of the {other} tokenizer")),
    }
    .context("the tokenizer's token is written out")?;
    indexed(a, predicate, &token)
}

/// The catalog of `tenant` the partition holds in memory, the store holding no catalog key: the catalog lives in the
/// metadata group.
fn stored_catalog(a: &TestChangesets, tenant: u64) -> TestResult<Catalog> {
    assert!(!holds_catalog_key(a), "no partition holds a catalog key");
    Ok((*a.catalog(tenant).context("the catalog")?).clone())
}

/// The keys the store holds of `tenant`, by keyspace: its catalog's, and every quad keyspace's.
fn tenant_keys(a: &TestChangesets, tenant: u64) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    stored_keys(a)
        .into_iter()
        .filter(|k| k.get(1..9) == Some(&tenant.to_be_bytes()[..]))
        .partition(|k| {
            matches!(
                k.first().and_then(|b| Keyspace::from_byte(*b)),
                Some((Keyspace::Catalog, _))
            )
        })
}

/// The predicates the subject-predicate list of `subject` holds, in the default graph.
fn subject_predicates(a: &TestChangesets, subject: &str) -> TestResult<Vec<u128>> {
    let key = QuadKey::SubjectPredicates {
        tenant: TENANT,
        graph: DEFAULT_GRAPH,
        subject: uid(a, subject)?,
    }
    .to_bytes();
    let latest = a.quads().lists().applied_ts();
    let list = a
        .quads()
        .lists()
        .read(&key, latest)
        .context("the list reads")?;
    let mut predicates = Vec::new();
    list.iterate(latest, 0, |p| {
        predicates.push(p.uid());
        Ok::<_, PostingError>(ControlFlow::Continue(()))
    })
    .context("the list iterates")?;
    Ok(predicates)
}

/// An ID as the steps carry it between them: its 32 hexadecimal digits.
fn hex(id: SystemId) -> String {
    format!("{:032x}", id.to_u128())
}

/// The ID `hex` wrote.
fn id_of_hex(text: &str) -> TestResult<SystemId> {
    Ok(SystemId::from_u128(u128::from_str_radix(text, 16)?)?)
}

/// Whether a stored key names predicate `predicate`.
fn names_predicate(key: &[u8], predicate: SystemId) -> bool {
    QuadKey::parse(key).is_ok_and(|(k, _)| k.predicate() == Some(predicate))
}

// Builds.

#[when("a predicate holds 1,000 literals and an `exact` index is declared on it")]
fn thousand_then_exact(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let changes = (0..1_000)
        .map(|i| insert(&format!("<s{i}> <name> \"v{i}\"")))
        .collect::<TestResult<_>>()?;
    let _applied: Applied = apply(&a, changes).context("the literals are written")?;
    declare(&a, "name", "with `exact`")?;
    assert!(a.rebuilding(), "the declaration starts a build");
    Ok(())
}

#[then("after the rebuild, the index holds a token for each of the 1,000 literals")]
fn thousand_indexed(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    finish_builds(&a);
    assert!(!a.rebuilding());
    for i in 0..1_000 {
        let subject = uid(&a, &format!("s{i}"))?;
        assert_eq!(
            listed(&a, "name", "exact", &format!("v{i}"))?,
            vec![subject]
        );
    }
    Ok(())
}

#[when(
    "a predicate indexed by `term` is declared with `exact` instead, and the `exact` index is being built"
)]
fn term_to_exact_building(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    declare(&a, "name", "with `term`")?;
    write(&a, &["<a> <name> \"big apple\""])?;
    declare(&a, "name", "with `exact`")?;
    Ok(())
}

#[then("the declaration used for queries reports the predicate with neither `term` nor `exact`")]
fn interim_reported(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    assert!(a.rebuilding(), "the build is under way");
    let p = uid(&a, "name")?;
    for catalog in [
        (*a.catalog(TENANT).context("the catalog")?).clone(),
        stored_catalog(&a, TENANT)?,
    ] {
        let declaration = catalog.predicate(p).context("`name` is declared")?;
        assert!(declaration.tokenizers().is_empty(), "{declaration:?}");
    }
    finish_builds(&a);
    assert_eq!(
        a.catalog(TENANT)
            .context("the catalog")?
            .predicate(p)
            .map(|d| d.tokenizers().to_vec()),
        Some(vec!["exact".to_string()])
    );
    Ok(())
}

#[when("a literal is written while its predicate's index is rebuilt")]
fn written_during_build(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    write(&a, &["<a> <name> \"big apple\""])?;
    declare(&a, "name", "with `exact`")?;
    write(&a, &["<b> <name> \"red pear\""])?;
    assert!(a.rebuilding(), "the build is under way");
    Ok(())
}

#[then("after the rebuild, the index holds the literal's tokens")]
fn written_during_build_indexed(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    finish_builds(&a);
    assert_eq!(
        listed(&a, "name", "exact", "red pear")?,
        vec![uid(&a, "b")?]
    );
    assert_eq!(
        listed(&a, "name", "exact", "big apple")?,
        vec![uid(&a, "a")?]
    );
    Ok(())
}

#[when(
    "`name` indexed by `term` is declared with `exact` instead, and `<b> <name> \"pear\"` is written while the `exact` index is built"
)]
fn term_to_exact_with_write(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    declare(&a, "name", "with `term`")?;
    write(&a, &["<a> <name> \"big apple\""])?;
    declare(&a, "name", "with `exact`")?;
    write(&a, &["<b> <name> \"pear\""])?;
    assert!(a.rebuilding(), "the build is under way");
    Ok(())
}

#[then("after the rebuild, no term key of `name` remains, and the exact key for `pear` holds `b`")]
fn term_gone_exact_held(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    finish_builds(&a);
    assert!(!has_index_keys(&a, "name", "term")?);
    assert_eq!(listed(&a, "name", "exact", "pear")?, vec![uid(&a, "b")?]);
    Ok(())
}

#[when("a predicate indexed by `term` and `exact` drops `term`")]
fn drops_term(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    declare(&a, "name", "with `term` and `exact`")?;
    write(&a, &["<a> <name> \"big apple\""])?;
    declare(&a, "name", "with `exact`")?;
    assert!(!a.rebuilding(), "removing an index builds nothing");
    Ok(())
}

#[then("its term keys are gone and its exact keys remain")]
fn term_keys_gone(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    assert!(!has_index_keys(&a, "name", "term")?);
    assert_eq!(
        listed(&a, "name", "exact", "big apple")?,
        vec![uid(&a, "a")?]
    );
    Ok(())
}

// Type changes.

#[when("`a` holds two objects of `nick` and `nick` is declared single-valued")]
fn nick_single(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    write(&a, &["<a> <nick> \"Al\"", "<a> <nick> \"Ally\""])?;
    w.results.push(declared(&a, "nick", "single-valued")?);
    Ok(())
}

#[when("`name` holds `\"Ann\"` for `a` and `name` is declared an edge")]
fn name_edge(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    write(&a, &["<a> <name> \"Ann\""])?;
    w.results.push(declared(&a, "name", "an edge")?);
    Ok(())
}

#[when("`age` holds only `xsd:integer` literals and is declared with datatype `xsd:integer`")]
fn age_integer(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    write(&a, &["<a> <age> 30", "<b> <age> 31"])?;
    w.names = vec!["age".into(), "with datatype `xsd:integer`".into()];
    w.results
        .push(declared(&a, "age", "with datatype `xsd:integer`")?);
    Ok(())
}

#[when("`friend` gained `<a> <friend> <b>` at 3 and lost it at 4, and is declared a property at 5")]
fn friend_gained_and_lost(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    write(&a, &["<a> <friend> <b>"])?;
    let _applied: Applied = apply(
        &a,
        vec![crate::utils::changesets::delete("<a> <friend> <b>")?],
    )
    .context("the delete applies")?;
    w.names = vec!["friend".into(), "a property".into()];
    w.results.push(declared(&a, "friend", "a property")?);
    Ok(())
}

#[then(
    regex = r"^the change fails with an error naming (`\w+`) and (`\w+`)(?:, and `(\w+)` stays multi-valued)?$"
)]
fn change_fails(w: &mut Steps, args: &[String]) -> TestResult {
    let predicate = &args[0];
    let subject = &args[1];
    let multi = &args[2];
    let refused = last(w)?.as_ref().err().context("the change fails")?;
    let error = refused.to_string();
    for name in [&predicate, &subject] {
        let shown = format!("<{}>", ex(name.trim_matches('`')).as_str());
        assert!(error.contains(&shown), "{error} names {shown}");
    }
    // A declaration the data contradicts, naming the predicate and the subject at fault in their fields too.
    let (p, s) = (
        ex(predicate.trim_matches('`')),
        format!("<{}>", ex(subject.trim_matches('`')).as_str()),
    );
    assert!(
        matches!(
            &refused.0,
            ChangesetErrorEnum::Contradicted { predicate: named, subject: holder, .. }
                if named == p.as_str() && *holder == s
        ),
        "{refused:?}"
    );
    if !multi.is_empty() {
        let a = cs(w)?;
        let catalog = a.catalog(TENANT).context("the catalog")?;
        let declared = catalog.predicate(uid(&a, multi)?);
        assert!(declared.is_none_or(|d| d.cardinality() == Cardinality::Multi));
    }
    Ok(())
}

#[then("the change is applied")]
fn change_applied(w: &mut Steps) -> TestResult {
    assert!(last(w)?.is_ok(), "{:?}", last(w)?);
    let a = cs(w)?;
    let expected = crate::utils::changesets::declaration(&w.names[0], &w.names[1])?;
    let p = uid(&a, &w.names[0])?;
    assert_eq!(
        a.catalog(TENANT).context("the catalog")?.predicate(p),
        Some(&expected)
    );
    assert_eq!(stored_catalog(&a, TENANT)?.predicate(p), Some(&expected));
    Ok(())
}

// Drops.

#[when("`knows` is held for `a` and `b` and is dropped")]
fn knows_dropped(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    write(
        &a,
        &[
            "<a> <knows> <c>",
            "<b> <knows> <c>",
            "<a> <name> \"Ann\"",
            "<b> <name> \"Bob\"",
        ],
    )?;
    let id = crate::utils::changesets::insert_edge("<a> <knows> <c>", &[("since", "2020")])?;
    let _applied: Applied = apply(&a, vec![id]).context("an edge with a statement ID applies")?;
    w.names = vec![hex(uid(&a, "knows")?)];
    let _dropped_at: u64 = a
        .drop_predicate(TENANT, ex("knows").as_ref())
        .context("the predicate drops")?;
    Ok(())
}

#[then(
    "no key of `knows` remains, and neither the subject predicates of `a` nor of `b` holds `knows`"
)]
fn knows_gone(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let knows = id_of_hex(w.names.first().context("an ID")?)?;
    assert!(!stored_keys(&a).iter().any(|k| names_predicate(k, knows)));
    assert!(
        !stored_keys(&a)
            .iter()
            .any(|k| matches!(Keyspace::from_byte(k[0]), Some((Keyspace::Statements, _)))),
        "the edge's statement record is gone"
    );
    let name = uid(&a, "name")?;
    for subject in ["a", "b"] {
        assert_eq!(
            subject_predicates(&a, subject)?,
            vec![name.to_u128()],
            "{subject}"
        );
    }
    assert!(stored_catalog(&a, TENANT)?.predicate(knows).is_none());
    Ok(())
}

#[when("`emb` is declared with a vector index, holds vectors, and is dropped")]
fn emb_dropped(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 3",
    )?;
    write(
        &a,
        &[
            "<a> <emb> \"[0.5, 1, 2]\"^^ugdb:float32vector",
            "<b> <emb> \"[1, 0.5, 2]\"^^ugdb:float32vector",
        ],
    )?;
    w.names = vec![hex(uid(&a, "emb")?)];
    let _dropped_at: u64 = a
        .drop_predicate(TENANT, ex("emb").as_ref())
        .context("the predicate drops")?;
    Ok(())
}

#[then("no forward or vector-index key of `emb` remains")]
fn emb_gone(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let emb = id_of_hex(w.names.first().context("an ID")?)?;
    assert!(!stored_keys(&a).iter().any(|k| names_predicate(k, emb)));
    Ok(())
}

#[when("graphs `<g1>` and `<g2>` hold statements and `<g1>` is dropped")]
fn g1_dropped(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    write(
        &a,
        &[
            "<a> <knows> <b> <g1>",
            "<a> <name> \"Ann\" <g1>",
            "<a> <knows> <c> <g2>",
            "<a> <name> \"Ann\" <g2>",
        ],
    )?;
    w.names = vec![hex(uid(&a, "g1")?)];
    let _dropped_at: u64 = a
        .drop_graph(TENANT, GraphName::from(ex("g1")).as_ref())
        .context("the graph drops")?;
    Ok(())
}

#[then("no key of `<g1>` remains, and every statement of `<g2>` is still held")]
fn g1_gone(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let g1 = ultragraphdb_core_keys::Graph::Named(id_of_hex(w.names.first().context("an ID")?)?);
    assert!(
        !stored_keys(&a)
            .iter()
            .any(|k| QuadKey::parse(k).is_ok_and(|(k, _)| k.graph() == Some(g1)))
    );
    for text in ["<a> <knows> <c> <g2>", "<a> <name> \"Ann\" <g2>"] {
        assert!(is_held(&a, text)?, "{text} is held");
    }
    Ok(())
}

#[when("a tenant's data is dropped")]
fn data_dropped(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    declare(&a, "name", "with `exact`")?;
    write(&a, &["<a> <name> \"Ann\"", "<a> <knows> <b>"])?;
    w.schemas = vec![format!("{:?}", stored_catalog(&a, TENANT)?)];
    let _dropped_at: u64 = a.drop_data(TENANT).context("the data drops")?;
    Ok(())
}

#[then("its predicates hold no data and its catalog is unchanged")]
fn data_gone_catalog_kept(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let (_, quads) = tenant_keys(&a, TENANT);
    assert!(quads.is_empty(), "{} quad keys remain", quads.len());
    assert_eq!(
        vec![format!("{:?}", stored_catalog(&a, TENANT)?)],
        w.schemas
    );
    assert_eq!(
        *a.catalog(TENANT).context("the catalog")?,
        stored_catalog(&a, TENANT)?
    );
    Ok(())
}

#[when("everything in tenant 3 is dropped and the roles are restarted")]
fn tenant_three_dropped(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    write(&a, &["<a> <name> \"Ann\""])?;
    let three = |change| Changeset::new(3).with(change);
    let _applied: Applied = a
        .apply(&three(insert("<a> <name> \"Three\"")?))
        .context("tenant 3 is written")?;
    let _declared: Applied = a
        .apply(&three(Change::Declare(
            ultragraphdb_core_mapping::Declaration::Predicate(
                crate::utils::changesets::declaration("name", "with `exact`")?,
            ),
        )))
        .context("tenant 3 declares")?;
    let _dropped_at: u64 = a.drop_tenant(3).context("the tenant drops")?;
    // The group's half: the frontend commits tenant 3's initial catalog as its next version.
    initial_version(w, 3, 2)?;
    let b = crate::schema_storage_steps::restarted(&w.source, &a, &[TENANT, 3])?;
    w.changesets = Some(Arc::new(b));
    Ok(())
}

/// Commits `tenant`'s initial catalog, a version declaring nothing above it, as version `version` to the group.
fn initial_version(w: &mut Steps, tenant: u64, version: u64) -> TestResult {
    let bytes =
        ultragraphdb_core_catalog::CatalogContents::new(tenant, version, Vec::new()).encode()?;
    w.source
        .commit(ultragraphdb_traits_catalog::HeldVersion::new(
            tenant, version, version, bytes,
        ));
    Ok(())
}

/// Whether the group holds `tenant`'s initial catalog as the version in effect.
fn group_holds_initial(w: &Steps, tenant: u64) -> TestResult<bool> {
    use ultragraphdb_traits_catalog::CatalogSource;
    let held = w
        .source
        .version_at(tenant, u64::MAX)?
        .context("a version")?;
    let contents = ultragraphdb_core_catalog::CatalogContents::decode(
        tenant,
        held.version(),
        held.contents(),
    )?;
    Ok(contents.declarations().is_empty())
}

#[then("tenant 3 holds no quad key, and the metadata group holds its initial catalog")]
fn tenant_three_initial(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let rdf_type = a
        .quads()
        .id_of(3, rdf::TYPE.as_str())
        .context("the dictionary reads")?
        .context("rdf:type has a UID in tenant 3")?;
    let (catalog, quads) = tenant_keys(&a, 3);
    assert!(quads.is_empty(), "{} quad keys remain", quads.len());
    assert!(catalog.is_empty(), "no catalog key");
    assert!(group_holds_initial(w, 3)?);
    let initial = Catalog::initial(3, rdf_type).with_version(2);
    assert_eq!(*a.catalog(3).context("the catalog")?, initial);
    assert!(
        is_held(&a, "<a> <name> \"Ann\"")?,
        "tenant 0 keeps its data"
    );
    Ok(())
}

/// What a tenant held before its neighbour's drop: its keys by keyspace, and its catalog.
pub(crate) type Neighbour = ((Vec<Vec<u8>>, Vec<Vec<u8>>), Arc<Catalog>);

#[when(
    "tenants 7 and 8 share a bucket run, each holds statements in one partition, and everything in tenant 7 is dropped"
)]
fn neighbour_dropped(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    for tenant in [7, 8] {
        let write = Changeset::new(tenant).with(insert("<a> <name> \"Both\"")?);
        let _applied: Applied = a.apply(&write).context("the tenant is written")?;
    }
    let held = (tenant_keys(&a, 8), a.catalog(8).context("the catalog")?);
    let _dropped_at: u64 = a.drop_tenant(7).context("the tenant drops")?;
    w.neighbour = Some(held);
    Ok(())
}

#[then(
    "tenant 7 holds no quad key, its catalog is the initial catalog, and every statement and the catalog of tenant 8 are unchanged"
)]
fn neighbour_kept(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let (keys, quads) = tenant_keys(&a, 7);
    assert!(keys.is_empty() && quads.is_empty(), "{quads:?}");
    let rdf_type = a
        .quads()
        .id_of(7, rdf::TYPE.as_str())
        .context("the dictionary reads")?
        .context("rdf:type has a UID in tenant 7")?;
    let catalog = a.catalog(7).context("the catalog")?;
    let initial = Catalog::initial(7, rdf_type).with_version(catalog.version());
    assert_eq!(*catalog, initial);
    let (held, held_catalog) = w.neighbour.take().context("what tenant 8 held")?;
    assert_eq!(tenant_keys(&a, 8), held);
    assert_eq!(*a.catalog(8).context("the catalog")?, *held_catalog);
    Ok(())
}

#[when("a tenant whose lists are split has its data dropped")]
fn split_data_dropped(w: &mut Steps) -> TestResult {
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    let executor = SteppedExecutor::new(Seed::new(1).stream(Stream::Executor));
    let lists = Lists::new(
        crate::utils::changesets::sim_store(),
        cache,
        ListsConfig::new(PersistMode::Buffer, 256),
    );
    let a = Arc::new(
        TestChangesets::new(
            lists,
            crate::utils::changesets::blocks(),
            ultragraphdb_core_mapping::RandomSource::new(1),
            executor,
            1,
        )
        .context("the applier opens")?,
    );
    w.changesets = Some(a.clone());
    declare(&a, "friend", "an edge")?;
    let changes = (0..3_000)
        .map(|i| insert(&format!("<a> <friend> <o{i}>")))
        .collect::<TestResult<_>>()?;
    let _applied: Applied = apply(&a, changes).context("the edges are written")?;
    let key = QuadKey::Forward {
        tenant: TENANT,
        graph: DEFAULT_GRAPH,
        predicate: uid(&a, "friend")?,
        subject: uid(&a, "a")?,
    }
    .to_bytes();
    let _rollup_key: bool = a
        .quads()
        .lists()
        .rollup_key(&key)
        .context("the list rolls up")?;
    // The rollup is written one above the commit it folds, past the watermark: a read of everything reaches it.
    let list = a
        .quads()
        .lists()
        .read(&key, u64::MAX)
        .context("the list reads")?;
    assert!(!list.plist().splits().is_empty(), "the list is split");
    w.schemas = vec![format!("{:?}", stored_catalog(&a, TENANT)?)];
    let _dropped_at: u64 = a.drop_data(TENANT).context("the data drops")?;
    Ok(())
}

#[then(
    "no key of any quad keyspace of the tenant remains, split or not, and its catalog is unchanged"
)]
fn split_data_gone(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    let (_, quads) = tenant_keys(&a, TENANT);
    assert!(quads.is_empty(), "{} quad keys remain", quads.len());
    assert_eq!(
        vec![format!("{:?}", stored_catalog(&a, TENANT)?)],
        w.schemas
    );
    Ok(())
}

// Failed builds and rollups across drops.

#[when("`term` is replaced by `exact` and the `exact` build fails")]
fn failed_build(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    declare(&a, "name", "with `term`")?;
    write(&a, &["<a> <name> \"big apple\"", "<b> <name> \"red pear\""])?;
    declare(&a, "name", "with `exact`")?;
    // A write during the build maintains both indexes.
    write(&a, &["<c> <name> \"green fig\""])?;
    a.quads().store().inject(Fault::FailNextScanAfter(0));
    finish_builds(&a);
    Ok(())
}

#[then(
    "once the restoring version takes effect the schema declares `term`, and every term key is present"
)]
fn old_index_kept(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    assert!(!a.rebuilding());
    let p = uid(&a, "name")?;
    // The partition reports the failure; the group commits a version restoring the previous declaration.
    let restoring = vec![ultragraphdb_core_mapping::Declaration::Predicate(
        crate::utils::changesets::declaration("name", "with `term`")?,
    )];
    let refusals = crate::schema_storage_steps::take_effect(&w.source, &a, TENANT, 3, restoring)?;
    assert!(refusals.is_empty(), "{refusals:?}");
    for catalog in [
        (*a.catalog(TENANT).context("the catalog")?).clone(),
        stored_catalog(&a, TENANT)?,
    ] {
        let declared = catalog.predicate(p).context("`name` is declared")?;
        assert_eq!(declared.tokenizers(), ["term"], "{declared:?}");
    }
    for (subject, value) in [("a", "big apple"), ("b", "red pear"), ("c", "green fig")] {
        for word in value.split(' ') {
            assert_eq!(
                listed(&a, "name", "term", word)?,
                vec![uid(&a, subject)?],
                "{word}"
            );
        }
    }
    assert!(!has_index_keys(&a, "name", "exact")?);
    Ok(())
}

#[when("a rollup of a `name` list reads at 7, `name` is dropped at 8, and the rollup then writes")]
fn rollup_across_drop(w: &mut Steps) -> TestResult {
    let probe = Probe::new(crate::utils::changesets::sim_store());
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    let executor = SteppedExecutor::new(Seed::new(1).stream(Stream::Executor));
    let lists = Lists::new(probe.clone(), cache, ListsConfig::default());
    let a = Arc::new(
        ChangesetApplier::new(
            lists,
            crate::utils::changesets::blocks(),
            ultragraphdb_core_mapping::RandomSource::new(1),
            executor,
            1,
        )
        .context("the applier opens")?,
    );
    let changeset =
        |text: &str| -> TestResult<Changeset> { Ok(Changeset::new(TENANT).with(insert(text)?)) };
    let _applied: Applied = a
        .apply(&changeset("<a> <name> \"A\"")?)
        .context("the write applies")?;
    let _rewritten: Applied = a
        .apply(&changeset("<a> <name> \"B\"")?)
        .context("the write applies")?;
    let name = a
        .quads()
        .id_of(TENANT, ex("name").as_str())
        .context("the dictionary reads")?
        .context("`name` has a UID")?;
    let subject = a
        .quads()
        .id_of(TENANT, ex("a").as_str())
        .context("the dictionary reads")?
        .context("`a` has a UID")?;
    let key = QuadKey::Forward {
        tenant: TENANT,
        graph: DEFAULT_GRAPH,
        predicate: name,
        subject,
    }
    .to_bytes();
    let dropped = Arc::new(Mutex::new(None));
    let (inner, record) = (Arc::clone(&a), Arc::clone(&dropped));
    probe.on_next_snapshot(Box::new(move || {
        let outcome = inner.drop_predicate(TENANT, ex("name").as_ref());
        *record.lock().unwrap_or_else(PoisonError::into_inner) = Some(outcome.is_ok());
    }));
    w.flags = vec![
        a.quads()
            .lists()
            .rollup_key(&key)
            .context("the rollup runs")?,
    ];
    w.flags.push(
        dropped
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .unwrap_or(false),
    );
    w.flags.push(
        probe
            .inner()
            .contents()
            .keys()
            .keys()
            .any(|k| names_predicate(k, name)),
    );
    Ok(())
}

#[then("its batch is discarded, and no key of `name` exists")]
fn rollup_discarded(w: &mut Steps) {
    // Dgraph's drops do not stop rollups, so the rollup writes the list back (ledger 38).
    assert_eq!(w.flags, vec![false, true, false]);
}

// Telemetry.

#[when("an index build running in the background fails")]
fn build_fails_logged(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    declare(&a, "name", "with `term`")?;
    write(&a, &["<a> <name> \"big apple\""])?;
    declare(&a, "name", "with `exact`")?;
    a.quads().store().inject(Fault::FailNextScanAfter(0));
    let capture = LogCapture::new();
    capture.capture(|| finish_builds(&a));
    w.events = capture.events();
    Ok(())
}

#[then(
    "the applier emits an error event `error in building indexes, aborting` with the build's record as a field"
)]
fn build_failure_logged(w: &mut Steps) {
    let failed: Vec<_> = w
        .events
        .iter()
        .filter(|e| e.message() == Some("error in building indexes, aborting"))
        .collect();
    assert_eq!(failed.len(), 1, "{:?}", w.events);
    assert_eq!(failed[0].level(), tracing::Level::ERROR);
    assert!(
        failed[0]
            .field("record")
            .is_some_and(|r| r.parse::<u64>().is_ok()),
        "{:?}",
        failed[0]
    );
}

#[when(
    "`nick` is declared single-valued while `a` holds one object of it, and a commit adding a second object of `nick` to `a` lands in the epoch before the version takes effect"
)]
fn contradiction_before_effect(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    write(&a, &["<a> <nick> \"x\""])?;
    // The version is pending while the second object commits; it takes effect at the next epoch, as its round.
    write(&a, &["<a> <nick> \"y\""])?;
    let single = vec![ultragraphdb_core_mapping::Declaration::Predicate(
        crate::utils::changesets::declaration("nick", "single-valued")?,
    )];
    w.refusals = crate::schema_storage_steps::take_effect(&w.source, &a, TENANT, 2, single)?;
    Ok(())
}

#[then(
    "the check at the effective epoch fails naming `nick` and `a`, the change's reply is that error, and `nick` stays multi-valued"
)]
fn contradiction_refused(w: &mut Steps) -> TestResult {
    let a = cs(w)?;
    assert_eq!(w.refusals.len(), 1, "{:?}", w.refusals);
    let reply = w.refusals.first().context("the change's reply")?;
    assert_eq!(
        reply,
        "The declaration of <http://ex.org/nick> is refused by its data: <http://ex.org/a> holds more than one object"
    );
    let catalog = a.catalog(TENANT)?;
    assert_eq!(catalog.version(), 2, "the version took effect");
    let nick = catalog.predicate(uid(&a, "nick")?);
    assert!(
        nick.is_none_or(|d| d.cardinality() == Cardinality::Multi),
        "{nick:?}"
    );
    Ok(())
}
