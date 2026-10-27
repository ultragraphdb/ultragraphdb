/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The steps of the catalog-change scenarios over a changeset applier: index builds, which run on its stepped
//! executor only when a step runs them, the declarations its data refuses, and drops.

use std::ops::ControlFlow;
use std::sync::{Arc, Mutex};

use cucumber::{then, when};
use oxrdf::GraphName;
use oxrdf::vocab::rdf;
use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_keys::{DEFAULT_GRAPH, Keyspace, QuadKey};
use ultragraphdb_core_mapping::{Cardinality, Change, Changeset};
use ultragraphdb_core_tok::get_tokenizer;
use ultragraphdb_store_apply::ChangesetApplier;
use ultragraphdb_store_posting::{Lists, ListsConfig, PostingError};
use ultragraphdb_store_sim::{Fault, LogCapture, Seed, SimListCache, SteppedExecutor, Stream};
use ultragraphdb_traits_store::{PersistMode, Store};

use crate::Steps;
use crate::changeset_steps::{cs, is_held, last};
use crate::utils::changesets::{
    NS, TestChangesets, apply, changesets_over, declare, declared, ex, exact_token, finish_builds,
    indexed, insert, stored_keys, term_token, uid, write,
};
use crate::utils::probe::Probe;

/// Whether the store holds a key of the value index of `predicate` under `tokenizer`.
fn has_index_keys(a: &TestChangesets, predicate: &str, tokenizer: &str) -> bool {
    let p = uid(a, predicate);
    let id = get_tokenizer(tokenizer).expect("a tokenizer").identifier();
    stored_keys(a).iter().any(|k| {
        matches!(
            QuadKey::parse(k),
            Ok((QuadKey::ValueIndex { predicate, tokenizer, .. }, _)) if predicate == p && tokenizer == id
        )
    })
}

/// The subjects the `tokenizer` index of `predicate` lists under `text`, in the default graph: the `exact` token of a
/// string, or the `term` token of a lowercase word, written out independently of the tokenizer.
fn listed(a: &TestChangesets, predicate: &str, tokenizer: &str, text: &str) -> Vec<u64> {
    let token = match tokenizer {
        "exact" => exact_token(text),
        "term" => term_token(text),
        other => panic!("no written-out token of the {other} tokenizer"),
    };
    indexed(a, predicate, &token)
}

/// The catalog of `namespace` as the store holds it at the newest timestamp.
fn stored_catalog(a: &TestChangesets, namespace: u64) -> Catalog {
    let latest = a.quads().lists().latest_ts();
    let snapshot = a.quads().store().snapshot(latest).expect("a snapshot");
    Catalog::load(&snapshot, namespace).expect("the catalog loads")
}

/// The keys the store holds of `namespace`, by keyspace: its catalog's, and every quad keyspace's.
fn namespace_keys(a: &TestChangesets, namespace: u64) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    stored_keys(a)
        .into_iter()
        .filter(|k| k.get(1..9) == Some(&namespace.to_be_bytes()[..]))
        .partition(|k| matches!(Keyspace::from_byte(k[0]), Some((Keyspace::Catalog, _))))
}

/// The predicates the subject-predicate list of `subject` holds, in the default graph.
fn subject_predicates(a: &TestChangesets, subject: &str) -> Vec<u64> {
    let key = QuadKey::SubjectPredicates {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        subject: uid(a, subject),
    }
    .to_bytes();
    let latest = a.quads().lists().latest_ts();
    let list = a
        .quads()
        .lists()
        .read(&key, latest)
        .expect("the list reads");
    let mut predicates = Vec::new();
    list.iterate(latest, 0, |p| {
        predicates.push(p.uid());
        Ok::<_, PostingError>(ControlFlow::Continue(()))
    })
    .expect("the list iterates");
    predicates
}

/// Whether a stored key names predicate `predicate`.
fn names_predicate(key: &[u8], predicate: u64) -> bool {
    QuadKey::parse(key).is_ok_and(|(k, _)| k.predicate() == Some(predicate))
}

// Builds.

#[when("a predicate holds 1,000 literals and an `exact` index is declared on it")]
fn thousand_then_exact(w: &mut Steps) {
    let a = cs(w);
    let changes = (0..1_000)
        .map(|i| insert(&format!("<s{i}> <name> \"v{i}\"")))
        .collect();
    apply(&a, changes).expect("the literals are written");
    declare(&a, "name", "with `exact`");
    assert!(a.rebuilding(), "the declaration starts a build");
}

#[then("after the rebuild, the index holds a token for each of the 1,000 literals")]
fn thousand_indexed(w: &mut Steps) {
    let a = cs(w);
    finish_builds(&a);
    assert!(!a.rebuilding());
    for i in 0..1_000 {
        let subject = uid(&a, &format!("s{i}"));
        assert_eq!(listed(&a, "name", "exact", &format!("v{i}")), vec![subject]);
    }
}

#[when(
    "a predicate indexed by `term` is declared with `exact` instead, and the `exact` index is being built"
)]
fn term_to_exact_building(w: &mut Steps) {
    let a = cs(w);
    declare(&a, "name", "with `term`");
    write(&a, &["<a> <name> \"big apple\""]);
    declare(&a, "name", "with `exact`");
}

#[then("the declaration used for queries reports the predicate with neither `term` nor `exact`")]
fn interim_reported(w: &mut Steps) {
    let a = cs(w);
    assert!(a.rebuilding(), "the build is under way");
    let p = uid(&a, "name");
    for catalog in [
        (*a.catalog(NS).expect("the catalog")).clone(),
        stored_catalog(&a, NS),
    ] {
        let declaration = catalog.predicate(p).expect("`name` is declared");
        assert!(declaration.tokenizers().is_empty(), "{declaration:?}");
    }
    finish_builds(&a);
    assert_eq!(
        a.catalog(NS)
            .expect("the catalog")
            .predicate(p)
            .map(|d| d.tokenizers().to_vec()),
        Some(vec!["exact".to_string()])
    );
}

#[when("a literal is written while its predicate's index is rebuilt")]
fn written_during_build(w: &mut Steps) {
    let a = cs(w);
    write(&a, &["<a> <name> \"big apple\""]);
    declare(&a, "name", "with `exact`");
    write(&a, &["<b> <name> \"red pear\""]);
    assert!(a.rebuilding(), "the build is under way");
}

#[then("after the rebuild, the index holds the literal's tokens")]
fn written_during_build_indexed(w: &mut Steps) {
    let a = cs(w);
    finish_builds(&a);
    assert_eq!(listed(&a, "name", "exact", "red pear"), vec![uid(&a, "b")]);
    assert_eq!(listed(&a, "name", "exact", "big apple"), vec![uid(&a, "a")]);
}

#[when(
    "`name` indexed by `term` is declared with `exact` instead, and `<b> <name> \"pear\"` is written while the `exact` index is built"
)]
fn term_to_exact_with_write(w: &mut Steps) {
    let a = cs(w);
    declare(&a, "name", "with `term`");
    write(&a, &["<a> <name> \"big apple\""]);
    declare(&a, "name", "with `exact`");
    write(&a, &["<b> <name> \"pear\""]);
    assert!(a.rebuilding(), "the build is under way");
}

#[then("after the rebuild, no term key of `name` remains, and the exact key for `pear` holds `b`")]
fn term_gone_exact_held(w: &mut Steps) {
    let a = cs(w);
    finish_builds(&a);
    assert!(!has_index_keys(&a, "name", "term"));
    assert_eq!(listed(&a, "name", "exact", "pear"), vec![uid(&a, "b")]);
}

#[when("a predicate indexed by `term` and `exact` drops `term`")]
fn drops_term(w: &mut Steps) {
    let a = cs(w);
    declare(&a, "name", "with `term` and `exact`");
    write(&a, &["<a> <name> \"big apple\""]);
    declare(&a, "name", "with `exact`");
    assert!(!a.rebuilding(), "removing an index builds nothing");
}

#[then("its term keys are gone and its exact keys remain")]
fn term_keys_gone(w: &mut Steps) {
    let a = cs(w);
    assert!(!has_index_keys(&a, "name", "term"));
    assert_eq!(listed(&a, "name", "exact", "big apple"), vec![uid(&a, "a")]);
}

// Type changes.

#[when("`a` holds two objects of `nick` and `nick` is declared single-valued")]
fn nick_single(w: &mut Steps) {
    let a = cs(w);
    write(&a, &["<a> <nick> \"Al\"", "<a> <nick> \"Ally\""]);
    w.results.push(declared(&a, "nick", "single-valued"));
}

#[when("`name` holds `\"Ann\"` for `a` and `name` is declared an edge")]
fn name_edge(w: &mut Steps) {
    let a = cs(w);
    write(&a, &["<a> <name> \"Ann\""]);
    w.results.push(declared(&a, "name", "an edge"));
}

#[when("`age` holds only `xsd:integer` literals and is declared with datatype `xsd:integer`")]
fn age_integer(w: &mut Steps) {
    let a = cs(w);
    write(&a, &["<a> <age> 30", "<b> <age> 31"]);
    w.names = vec!["age".into(), "with datatype `xsd:integer`".into()];
    w.results
        .push(declared(&a, "age", "with datatype `xsd:integer`"));
}

#[when("`friend` gained `<a> <friend> <b>` at 3 and lost it at 4, and is declared a property at 5")]
fn friend_gained_and_lost(w: &mut Steps) {
    let a = cs(w);
    write(&a, &["<a> <friend> <b>"]);
    apply(
        &a,
        vec![crate::utils::changesets::delete("<a> <friend> <b>")],
    )
    .expect("the delete applies");
    w.names = vec!["friend".into(), "a property".into()];
    w.results.push(declared(&a, "friend", "a property"));
}

#[then(
    regex = r"^the change fails with an error naming (`\w+`) and (`\w+`)(?:, and `(\w+)` stays multi-valued)?$"
)]
fn change_fails(w: &mut Steps, predicate: String, subject: String, multi: String) {
    let error = last(w).as_ref().expect_err("the change fails").to_string();
    for name in [predicate, subject] {
        let shown = format!("<{}>", ex(name.trim_matches('`')).as_str());
        assert!(error.contains(&shown), "{error} names {shown}");
    }
    if !multi.is_empty() {
        let a = cs(w);
        let catalog = a.catalog(NS).expect("the catalog");
        let declared = catalog.predicate(uid(&a, &multi));
        assert!(declared.is_none_or(|d| d.cardinality() == Cardinality::Multi));
    }
}

#[then("the change is applied")]
fn change_applied(w: &mut Steps) {
    assert!(last(w).is_ok(), "{:?}", last(w));
    let a = cs(w);
    let expected = crate::utils::changesets::declaration(&w.names[0], &w.names[1]);
    let p = uid(&a, &w.names[0]);
    assert_eq!(
        a.catalog(NS).expect("the catalog").predicate(p),
        Some(&expected)
    );
    assert_eq!(stored_catalog(&a, NS).predicate(p), Some(&expected));
}

// Drops.

#[when("`knows` is held for `a` and `b` and is dropped")]
fn knows_dropped(w: &mut Steps) {
    let a = cs(w);
    write(
        &a,
        &[
            "<a> <knows> <c>",
            "<b> <knows> <c>",
            "<a> <name> \"Ann\"",
            "<b> <name> \"Bob\"",
        ],
    );
    let id = crate::utils::changesets::insert_edge("<a> <knows> <c>", &[("since", "2020")]);
    apply(&a, vec![id]).expect("an edge with a statement ID applies");
    w.names = vec![uid(&a, "knows").to_string()];
    a.drop_predicate(NS, ex("knows").as_ref())
        .expect("the predicate drops");
}

#[then(
    "no key of `knows` remains, and neither the subject predicates of `a` nor of `b` holds `knows`"
)]
fn knows_gone(w: &mut Steps) {
    let a = cs(w);
    let knows: u64 = w.names[0].parse().expect("a UID");
    assert!(!stored_keys(&a).iter().any(|k| names_predicate(k, knows)));
    assert!(
        !stored_keys(&a)
            .iter()
            .any(|k| matches!(Keyspace::from_byte(k[0]), Some((Keyspace::Statements, _)))),
        "the edge's statement record is gone"
    );
    let name = uid(&a, "name");
    for subject in ["a", "b"] {
        assert_eq!(subject_predicates(&a, subject), vec![name], "{subject}");
    }
    assert!(stored_catalog(&a, NS).predicate(knows).is_none());
}

#[when("`emb` is declared with a vector index, holds vectors, and is dropped")]
fn emb_dropped(w: &mut Steps) {
    let a = cs(w);
    declare(&a, "emb", "single-valued with a vector index");
    write(
        &a,
        &[
            "<a> <emb> \"[0.5, 1, 2]\"^^ugdb:float32vector",
            "<b> <emb> \"[1, 0.5, 2]\"^^ugdb:float32vector",
        ],
    );
    w.names = vec![uid(&a, "emb").to_string()];
    a.drop_predicate(NS, ex("emb").as_ref())
        .expect("the predicate drops");
}

#[then("no forward or vector-index key of `emb` remains")]
fn emb_gone(w: &mut Steps) {
    let a = cs(w);
    let emb: u64 = w.names[0].parse().expect("a UID");
    assert!(!stored_keys(&a).iter().any(|k| names_predicate(k, emb)));
}

#[when("graphs `<g1>` and `<g2>` hold statements and `<g1>` is dropped")]
fn g1_dropped(w: &mut Steps) {
    let a = cs(w);
    write(
        &a,
        &[
            "<a> <knows> <b> <g1>",
            "<a> <name> \"Ann\" <g1>",
            "<a> <knows> <c> <g2>",
            "<a> <name> \"Ann\" <g2>",
        ],
    );
    w.names = vec![uid(&a, "g1").to_string()];
    a.drop_graph(NS, GraphName::from(ex("g1")).as_ref())
        .expect("the graph drops");
}

#[then("no key of `<g1>` remains, and every statement of `<g2>` is still held")]
fn g1_gone(w: &mut Steps) {
    let a = cs(w);
    let g1: u64 = w.names[0].parse().expect("a UID");
    assert!(
        !stored_keys(&a)
            .iter()
            .any(|k| QuadKey::parse(k).is_ok_and(|(k, _)| k.graph() == Some(g1)))
    );
    for text in ["<a> <knows> <c> <g2>", "<a> <name> \"Ann\" <g2>"] {
        assert!(is_held(&a, text), "{text} is held");
    }
}

#[when("a namespace's data is dropped")]
fn data_dropped(w: &mut Steps) {
    let a = cs(w);
    declare(&a, "name", "with `exact`");
    write(&a, &["<a> <name> \"Ann\"", "<a> <knows> <b>"]);
    w.schemas = vec![format!("{:?}", stored_catalog(&a, NS))];
    a.drop_data(NS).expect("the data drops");
}

#[then("its predicates hold no data and its catalog is unchanged")]
fn data_gone_catalog_kept(w: &mut Steps) {
    let a = cs(w);
    let (_, quads) = namespace_keys(&a, NS);
    assert!(quads.is_empty(), "{} quad keys remain", quads.len());
    assert_eq!(vec![format!("{:?}", stored_catalog(&a, NS))], w.schemas);
    assert_eq!(*a.catalog(NS).expect("the catalog"), stored_catalog(&a, NS));
}

#[when("everything in namespace 3 is dropped and the store is reopened")]
fn namespace_three_dropped(w: &mut Steps) {
    let a = cs(w);
    write(&a, &["<a> <name> \"Ann\""]);
    let three = |change| Changeset::new(3).with(change);
    a.apply(&three(insert("<a> <name> \"Three\"")))
        .expect("namespace 3 is written");
    a.apply(&three(Change::Declare(
        ultragraphdb_core_mapping::Declaration::Predicate(crate::utils::changesets::declaration(
            "name",
            "with `exact`",
        )),
    )))
    .expect("namespace 3 declares");
    a.drop_namespace(3).expect("the namespace drops");
    let store = a.quads().store().clone();
    store.crash();
    store.recover();
    w.changesets = Some(Arc::new(changesets_over(store)));
}

#[then("namespace 3 holds only its initial catalog")]
fn namespace_three_initial(w: &mut Steps) {
    let a = cs(w);
    let rdf_type = a
        .quads()
        .uid_of(3, rdf::TYPE.as_str())
        .expect("the dictionary reads")
        .expect("rdf:type has a UID in namespace 3");
    let (catalog, quads) = namespace_keys(&a, 3);
    assert!(quads.is_empty(), "{} quad keys remain", quads.len());
    let initial = Catalog::initial(3, rdf_type);
    let mut expected: Vec<Vec<u8>> = initial
        .entries(1)
        .iter()
        .map(|e| e.key().to_vec())
        .collect();
    expected.sort();
    assert_eq!(catalog, expected);
    assert_eq!(*a.catalog(3).expect("the catalog"), initial);
    assert!(
        is_held(&a, "<a> <name> \"Ann\""),
        "namespace 0 keeps its data"
    );
}

#[when("everything is dropped")]
fn everything_dropped(w: &mut Steps) {
    let a = cs(w);
    declare(&a, "name", "with `exact`");
    write(&a, &["<a> <name> \"Ann\"", "<a> <knows> <b> <g1>"]);
    a.apply(&Changeset::new(3).with(insert("<a> <name> \"Three\"")))
        .expect("namespace 3 is written");
    a.drop_all().expect("everything drops");
}

#[then(
    "the store holds no quad key but the root namespace's initial catalog, written at timestamp 1"
)]
fn initial_catalog_only(w: &mut Steps) {
    let a = cs(w);
    let rdf_type = a
        .quads()
        .uid_of(NS, rdf::TYPE.as_str())
        .expect("the dictionary reads")
        .expect("the dictionary keeps rdf:type");
    let contents = a.quads().store().contents();
    let mut expected: Vec<Vec<u8>> = Catalog::initial(NS, rdf_type)
        .entries(1)
        .iter()
        .map(|e| e.key().to_vec())
        .collect();
    expected.sort();
    let keys: Vec<Vec<u8>> = contents.keys().keys().cloned().collect();
    assert_eq!(keys, expected);
    assert!(
        contents
            .keys()
            .values()
            .all(|v| v.len() == 1 && v[0].version() == 1)
    );
}

#[when("a namespace whose lists are split has its data dropped")]
fn split_data_dropped(w: &mut Steps) {
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    let executor = SteppedExecutor::new(Seed::new(1).stream(Stream::Executor));
    let lists = Lists::new(
        crate::utils::changesets::sim_store(),
        cache,
        ListsConfig::new(PersistMode::Buffer, 256),
    );
    let a = Arc::new(TestChangesets::new(lists, executor, 1).expect("the applier opens"));
    w.changesets = Some(a.clone());
    declare(&a, "friend", "an edge");
    let changes = (0..3_000)
        .map(|i| insert(&format!("<a> <friend> <o{i}>")))
        .collect();
    apply(&a, changes).expect("the edges are written");
    let key = QuadKey::Forward {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: uid(&a, "friend"),
        subject: uid(&a, "a"),
    }
    .to_bytes();
    a.quads()
        .lists()
        .rollup_key(&key)
        .expect("the list rolls up");
    let latest = a.quads().lists().latest_ts();
    let list = a
        .quads()
        .lists()
        .read(&key, latest)
        .expect("the list reads");
    assert!(!list.plist().splits().is_empty(), "the list is split");
    w.keys = namespace_keys(&a, NS).0;
    a.drop_data(NS).expect("the data drops");
}

#[then(
    "no key of any quad keyspace of the namespace remains, split or not, and its catalog keys are unchanged"
)]
fn split_data_gone(w: &mut Steps) {
    let a = cs(w);
    let (catalog, quads) = namespace_keys(&a, NS);
    assert!(quads.is_empty(), "{} quad keys remain", quads.len());
    assert_eq!(catalog, w.keys);
}

// Failed builds and rollups across drops.

#[when("`term` is replaced by `exact` and the `exact` build fails")]
fn failed_build(w: &mut Steps) {
    let a = cs(w);
    declare(&a, "name", "with `term`");
    write(&a, &["<a> <name> \"big apple\"", "<b> <name> \"red pear\""]);
    declare(&a, "name", "with `exact`");
    // A write during the build maintains both indexes.
    write(&a, &["<c> <name> \"green fig\""]);
    a.quads().store().inject(Fault::FailNextScanAfter(0));
    finish_builds(&a);
}

#[then("the schema declares `term` and every term key is present")]
fn old_index_kept(w: &mut Steps) {
    let a = cs(w);
    assert!(!a.rebuilding());
    let p = uid(&a, "name");
    for catalog in [
        (*a.catalog(NS).expect("the catalog")).clone(),
        stored_catalog(&a, NS),
    ] {
        let declared = catalog.predicate(p).expect("`name` is declared");
        assert_eq!(declared.tokenizers(), ["term"], "{declared:?}");
    }
    for (subject, value) in [("a", "big apple"), ("b", "red pear"), ("c", "green fig")] {
        for word in value.split(' ') {
            assert_eq!(
                listed(&a, "name", "term", word),
                vec![uid(&a, subject)],
                "{word}"
            );
        }
    }
    assert!(!has_index_keys(&a, "name", "exact"));
}

#[when("a rollup of a `name` list reads at 7, `name` is dropped at 8, and the rollup then writes")]
fn rollup_across_drop(w: &mut Steps) {
    let probe = Probe::new(crate::utils::changesets::sim_store());
    let cache = SimListCache::new(1024, Seed::new(1).stream(Stream::Cache));
    let executor = SteppedExecutor::new(Seed::new(1).stream(Stream::Executor));
    let lists = Lists::new(probe.clone(), cache, ListsConfig::default());
    let a = Arc::new(ChangesetApplier::new(lists, executor, 1).expect("the applier opens"));
    let changeset = |text: &str| Changeset::new(NS).with(insert(text));
    a.apply(&changeset("<a> <name> \"A\""))
        .expect("the write applies");
    a.apply(&changeset("<a> <name> \"B\""))
        .expect("the write applies");
    let name = a
        .quads()
        .uid_of(NS, ex("name").as_str())
        .expect("the dictionary reads")
        .expect("`name` has a UID");
    let subject = a
        .quads()
        .uid_of(NS, ex("a").as_str())
        .expect("the dictionary reads")
        .expect("`a` has a UID");
    let key = QuadKey::Forward {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: name,
        subject,
    }
    .to_bytes();
    let dropped = Arc::new(Mutex::new(None));
    let (inner, record) = (Arc::clone(&a), Arc::clone(&dropped));
    probe.on_next_snapshot(Box::new(move || {
        let outcome = inner.drop_predicate(NS, ex("name").as_ref());
        *record.lock().expect("the record") = Some(outcome.is_ok());
    }));
    w.flags = vec![a.quads().lists().rollup_key(&key).expect("the rollup runs")];
    w.flags
        .push(dropped.lock().expect("the record").unwrap_or(false));
    w.flags.push(
        probe
            .inner()
            .contents()
            .keys()
            .keys()
            .any(|k| names_predicate(k, name)),
    );
}

#[then("its batch is discarded, and no key of `name` exists")]
fn rollup_discarded(w: &mut Steps) {
    // Dgraph's drops do not stop rollups, so the rollup writes the list back (ledger 38).
    assert_eq!(w.flags, vec![false, true, false]);
}

// Telemetry.

#[when("an index build running in the background fails")]
fn build_fails_logged(w: &mut Steps) {
    let a = cs(w);
    declare(&a, "name", "with `term`");
    write(&a, &["<a> <name> \"big apple\""]);
    declare(&a, "name", "with `exact`");
    a.quads().store().inject(Fault::FailNextScanAfter(0));
    let capture = LogCapture::new();
    capture.capture(|| finish_builds(&a));
    w.events = capture.events();
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
