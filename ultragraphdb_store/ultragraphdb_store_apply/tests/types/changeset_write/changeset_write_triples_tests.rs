/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Inserts and deletes of triples at their corner cases: an empty changeset, one quad twice in a changeset, a set and
//! a delete of one quad in a changeset, a subject that is its own object, the empty string, the largest UIDs, deletes
//! of what is not held, and the checks a write's object meets. Expected counts are hand-counted from each test's
//! statements.

use oxrdf::{GraphName, NamedOrBlankNode};
use ultragraphdb_core_keys::{CountDirection, Keyspace};
use ultragraphdb_core_mapping::Change;
use ultragraphdb_store_apply::ChangesetErrorEnum;
use ultragraphdb_store_quads::{MAX_UID, QuadErrorEnum, UID_LEASE, lease_key};
use ultragraphdb_traits_store::{PersistMode, Store, WriteBatch};

use crate::utils::changesets::{
    NS, apply, changesets, changesets_over, counts_of, declare, delete, ex, exact_token,
    index_keys_holding, indexed, insert, keys_of, objects, objects_in, predicate_list, reverse,
    sim_store, statistics, stored_keys, term, term_token, uid,
};

/// The keys the store holds that are not catalog keys.
fn quad_keys(a: &crate::utils::changesets::TestChangesets) -> Vec<Vec<u8>> {
    stored_keys(a)
        .into_iter()
        .filter(|k| !matches!(Keyspace::from_byte(k[0]), Some((Keyspace::Catalog, _))))
        .collect()
}

#[test]
fn test_an_empty_changeset_commits_and_writes_no_quad() {
    let a = changesets();
    let first = apply(&a, Vec::new()).expect("an empty changeset applies");
    assert!(
        first.read().is_empty() && first.lists_read() == 0,
        "{first:?}"
    );
    assert!(quad_keys(&a).is_empty(), "{:?}", quad_keys(&a));
    let before = a.quads().store().contents();
    let second = apply(&a, Vec::new()).expect("an empty changeset applies");
    assert!(
        second.written().is_empty() && second.read().is_empty(),
        "{second:?}"
    );
    assert_eq!(a.quads().store().contents().keys(), before.keys());
}

#[test]
fn test_one_quad_twice_in_a_changeset_is_held_and_counted_once() {
    let a = changesets();
    declare(&a, "friend", "with an exact count index");
    declare(&a, "tag", "with a `term` index");
    apply(
        &a,
        vec![
            insert("<a> <friend> <b>"),
            insert("<a> <friend> <b>"),
            insert(r#"<a> <tag> "x""#),
            insert(r#"<a> <tag> "x""#),
        ],
    )
    .unwrap();
    assert_eq!(objects(&a, "a", "friend"), vec![term("<b>")]);
    assert_eq!(objects(&a, "a", "tag"), vec![term(r#""x""#)]);
    // One statement each, counted once: forward count 1 for `a`, reverse count 1 for `b`.
    assert_eq!(statistics(&a, "friend").statements(), 1);
    assert_eq!(statistics(&a, "tag").statements(), 1);
    assert_eq!(
        counts_of(&a, "friend", CountDirection::Forward, uid(&a, "a")),
        vec![1]
    );
    assert_eq!(
        counts_of(&a, "friend", CountDirection::Reverse, uid(&a, "b")),
        vec![1]
    );
    assert_eq!(reverse(&a, "friend", "b"), vec![uid(&a, "a")]);
    assert_eq!(indexed(&a, "tag", &term_token("x")), vec![uid(&a, "a")]);
}

#[test]
fn test_a_set_and_a_delete_of_one_quad_in_a_changeset_leave_nothing() {
    let a = changesets();
    declare(&a, "friend", "with an exact count index");
    declare(&a, "tag", "with a `term` index");
    apply(
        &a,
        vec![
            insert("<a> <friend> <b>"),
            delete("<a> <friend> <b>"),
            insert(r#"<a> <tag> "x""#),
            delete(r#"<a> <tag> "x""#),
        ],
    )
    .unwrap();
    assert!(objects(&a, "a", "friend").is_empty());
    assert!(objects(&a, "a", "tag").is_empty());
    assert!(reverse(&a, "friend", "b").is_empty());
    assert!(counts_of(&a, "friend", CountDirection::Forward, uid(&a, "a")).is_empty());
    assert!(counts_of(&a, "friend", CountDirection::Reverse, uid(&a, "b")).is_empty());
    assert!(index_keys_holding(&a, "tag", uid(&a, "a")).is_empty());
    assert!(predicate_list(&a, "a", false).is_empty());
    assert!(predicate_list(&a, "b", true).is_empty());
    assert_eq!(statistics(&a, "friend").statements(), 0);
    assert_eq!(statistics(&a, "tag").statements(), 0);
}

#[test]
fn test_a_delete_and_a_set_of_one_quad_in_a_changeset_hold_it_once() {
    let a = changesets();
    declare(&a, "friend", "with an exact count index");
    apply(&a, vec![insert("<a> <friend> <b>")]).unwrap();
    apply(
        &a,
        vec![delete("<a> <friend> <b>"), insert("<a> <friend> <b>")],
    )
    .unwrap();
    assert_eq!(objects(&a, "a", "friend"), vec![term("<b>")]);
    assert_eq!(reverse(&a, "friend", "b"), vec![uid(&a, "a")]);
    assert_eq!(
        counts_of(&a, "friend", CountDirection::Forward, uid(&a, "a")),
        vec![1]
    );
    assert_eq!(
        counts_of(&a, "friend", CountDirection::Reverse, uid(&a, "b")),
        vec![1]
    );
    assert_eq!(statistics(&a, "friend").statements(), 1);
}

#[test]
fn test_a_subject_that_is_its_own_object_is_kept_in_both_directions() {
    let a = changesets();
    declare(&a, "friend", "with an exact count index");
    apply(&a, vec![insert("<a> <friend> <a>")]).unwrap();
    let (node, friend) = (uid(&a, "a"), uid(&a, "friend"));
    assert_eq!(reverse(&a, "friend", "a"), vec![node]);
    assert_eq!(predicate_list(&a, "a", false), vec![friend]);
    assert_eq!(predicate_list(&a, "a", true), vec![friend]);
    // One object for the subject and one subject for the object, both the node itself.
    assert_eq!(
        counts_of(&a, "friend", CountDirection::Forward, node),
        vec![1]
    );
    assert_eq!(
        counts_of(&a, "friend", CountDirection::Reverse, node),
        vec![1]
    );
    apply(&a, vec![delete("<a> <friend> <a>")]).unwrap();
    assert!(reverse(&a, "friend", "a").is_empty());
    assert!(predicate_list(&a, "a", false).is_empty());
    assert!(predicate_list(&a, "a", true).is_empty());
    assert!(counts_of(&a, "friend", CountDirection::Forward, node).is_empty());
    assert!(counts_of(&a, "friend", CountDirection::Reverse, node).is_empty());
}

#[test]
fn test_the_empty_string_is_held_and_indexed() {
    let a = changesets();
    declare(&a, "name", "with `exact` and `term`");
    apply(&a, vec![insert(r#"<a> <name> """#)]).unwrap();
    assert_eq!(objects(&a, "a", "name"), vec![term(r#""""#)]);
    // The exact token of "" is the identifier alone; Dgraph's term tokenizer gives an empty value one empty token.
    assert_eq!(indexed(&a, "name", &exact_token("")), vec![uid(&a, "a")]);
    assert_eq!(indexed(&a, "name", &term_token("")), vec![uid(&a, "a")]);
    apply(&a, vec![delete(r#"<a> <name> """#)]).unwrap();
    assert!(objects(&a, "a", "name").is_empty());
    assert!(index_keys_holding(&a, "name", uid(&a, "a")).is_empty());
}

#[test]
fn test_a_single_valued_overwrite_moves_reverse_and_counts() {
    let a = changesets();
    declare(&a, "friend", "single-valued with an exact count index");
    apply(&a, vec![insert("<a> <friend> <b>")]).unwrap();
    apply(&a, vec![insert("<a> <friend> <c>")]).unwrap();
    let node = uid(&a, "a");
    assert_eq!(objects(&a, "a", "friend"), vec![term("<c>")]);
    assert!(reverse(&a, "friend", "b").is_empty());
    assert_eq!(reverse(&a, "friend", "c"), vec![node]);
    assert_eq!(
        counts_of(&a, "friend", CountDirection::Forward, node),
        vec![1]
    );
    assert!(counts_of(&a, "friend", CountDirection::Reverse, uid(&a, "b")).is_empty());
    assert_eq!(
        counts_of(&a, "friend", CountDirection::Reverse, uid(&a, "c")),
        vec![1]
    );
    assert_eq!(statistics(&a, "friend").statements(), 1);
    assert!(!predicate_list(&a, "b", true).contains(&uid(&a, "friend")));
}

#[test]
fn test_statistics_count_each_object_kind() {
    let a = changesets();
    apply(
        &a,
        vec![
            insert("<a> <p> <b>"),
            insert("<a> <p> _:z"),
            insert(r#"<a> <p> "x""#),
            insert("<a> <p> 5"),
        ],
    )
    .unwrap();
    let s = statistics(&a, "p");
    assert_eq!(s.statements(), 4);
    for (kind, n) in [
        ("iri", 1),
        ("blank", 1),
        ("http://www.w3.org/2001/XMLSchema#string", 1),
        ("http://www.w3.org/2001/XMLSchema#integer", 1),
    ] {
        assert_eq!(s.kind(kind), n, "{kind}");
    }
    apply(&a, vec![delete(r#"<a> <p> "x""#)]).unwrap();
    let s = statistics(&a, "p");
    assert_eq!(s.statements(), 3);
    assert_eq!(s.kind("http://www.w3.org/2001/XMLSchema#string"), 0);
    assert_eq!(s.kind("iri"), 1);
}

#[test]
fn test_a_delete_of_an_object_not_held_writes_nothing() {
    let a = changesets();
    declare(&a, "name", "with a `term` index");
    apply(&a, vec![insert(r#"<a> <name> "A""#)]).unwrap();
    let before = a.quads().store().contents();
    let applied = apply(&a, vec![delete(r#"<a> <name> "B""#)]).unwrap();
    assert!(applied.written().is_empty(), "{:?}", applied.written());
    assert_eq!(a.quads().store().contents().keys(), before.keys());
    assert_eq!(objects(&a, "a", "name"), vec![term(r#""A""#)]);
}

#[test]
fn test_a_delete_by_pattern_stays_in_its_graph() {
    let a = changesets();
    apply(
        &a,
        vec![
            insert("<a> <knows> <b> <g1>"),
            insert("<a> <knows> <c> <g1>"),
            insert("<a> <knows> <d>"),
        ],
    )
    .unwrap();
    apply(
        &a,
        vec![Change::DeletePattern {
            graph: GraphName::from(ex("g1")),
            subject: NamedOrBlankNode::from(ex("a")),
            predicate: ex("knows"),
        }],
    )
    .unwrap();
    assert!(objects_in(&a, "g1", "a", "knows").is_empty());
    assert_eq!(objects(&a, "a", "knows"), vec![term("<d>")]);
}

#[test]
fn test_a_unique_value_held_by_another_subject_is_refused_naming_both() {
    let a = changesets();
    declare(&a, "id", "unique");
    apply(&a, vec![insert(r#"<a> <id> "x""#)]).unwrap();
    let refused = apply(&a, vec![insert(r#"<b> <id> "x""#)]).unwrap_err();
    assert!(
        matches!(refused.0, ChangesetErrorEnum::Unique { .. }),
        "{refused:?}"
    );
    assert_eq!(
        refused.to_string(),
        r#"Predicate <http://ex.org/id> holds unique values, and <http://ex.org/a> already holds "x""#
    );
    // Another value, and the same value in another graph, are written.
    apply(&a, vec![insert(r#"<b> <id> "y""#)]).unwrap();
    apply(&a, vec![insert(r#"<b> <id> "x" <g1>"#)]).unwrap();
    assert_eq!(objects(&a, "b", "id"), vec![term(r#""y""#)]);
}

#[test]
fn test_an_object_of_the_wrong_kind_or_datatype_is_refused_with_its_text() {
    let a = changesets();
    declare(&a, "name", "a property");
    declare(&a, "age", "with datatype `xsd:integer`");
    let refused = apply(&a, vec![insert("<a> <name> <b>")]).unwrap_err();
    assert_eq!(
        refused.to_string(),
        "Predicate <http://ex.org/name> is declared a property and cannot hold <http://ex.org/b>"
    );
    let refused = apply(&a, vec![insert(r#"<a> <age> "old""#)]).unwrap_err();
    assert_eq!(
        refused.to_string(),
        "Predicate <http://ex.org/age> holds literals of <http://www.w3.org/2001/XMLSchema#integer>, not of <http://www.w3.org/2001/XMLSchema#string>"
    );
    assert!(keys_of(&a, Keyspace::Forward, "name").is_empty());
    assert!(keys_of(&a, Keyspace::Forward, "age").is_empty());
}

#[test]
fn test_the_largest_uids_are_written_and_the_next_is_refused() {
    let store = sim_store();
    // The lease mark is the next UID to hand out: MAX_UID - 3 = 2^48 - 4.
    let mut batch = WriteBatch::new();
    batch.put_versionless(
        lease_key(NS, UID_LEASE),
        (MAX_UID - 3).to_be_bytes().to_vec(),
    );
    store.apply(batch, PersistMode::Sync).unwrap();
    let a = changesets_over(store);
    // rdf:type, then <a>, <knows> and <b>, in the order the write resolves them.
    apply(&a, vec![insert("<a> <knows> <b>")]).unwrap();
    assert_eq!(
        (uid(&a, "a"), uid(&a, "knows"), uid(&a, "b")),
        (MAX_UID - 2, MAX_UID - 1, MAX_UID)
    );
    assert_eq!(reverse(&a, "knows", "b"), vec![MAX_UID - 2]);
    assert_eq!(objects(&a, "a", "knows"), vec![term("<b>")]);
    assert_eq!(predicate_list(&a, "b", true), vec![MAX_UID - 1]);
    let refused = apply(&a, vec![insert("<a> <knows> <c>")]).unwrap_err();
    assert!(
        matches!(&refused.0, ChangesetErrorEnum::Quad(q) if matches!(q.0, QuadErrorEnum::UidsExhausted { namespace: NS })),
        "{refused:?}"
    );
    assert_eq!(
        refused.to_string(),
        "Namespace 0 has no UID left below the entity-type bits"
    );
    assert_eq!(objects(&a, "a", "knows"), vec![term("<b>")]);
}

#[test]
fn test_a_write_in_another_namespace_keeps_its_keys_and_catalog_there() {
    let a = changesets();
    let three = |change| ultragraphdb_core_mapping::Changeset::new(3).with(change);
    a.apply(&three(Change::Declare(
        ultragraphdb_core_mapping::Declaration::Predicate(crate::utils::changesets::declaration(
            "name",
            "with `term` and an exact count index",
        )),
    )))
    .unwrap();
    a.apply(&three(insert(r#"<a> <name> "big apple""#)))
        .unwrap();
    let namespace_of = |k: &Vec<u8>| u64::from_be_bytes(k[1..9].try_into().unwrap());
    let keys = stored_keys(&a);
    assert!(!keys.is_empty());
    assert!(
        keys.iter().all(|k| namespace_of(k) == 3),
        "every key is namespace 3's"
    );
    for keyspace in [
        Keyspace::ValueIndex,
        Keyspace::CountIndex,
        Keyspace::Catalog,
    ] {
        assert!(
            keys.iter()
                .any(|k| matches!(Keyspace::from_byte(k[0]), Some((ks, _)) if ks == keyspace)),
            "{keyspace:?}"
        );
    }
    let name = a.quads().uid_of(3, ex("name").as_str()).unwrap().unwrap();
    a.forget_catalogs();
    assert_eq!(
        a.catalog(3).unwrap().predicate(name),
        Some(&crate::utils::changesets::declaration(
            "name",
            "with `term` and an exact count index"
        ))
    );
    assert!(a.catalog(NS).unwrap().predicates().next().is_none());
}

#[test]
fn test_a_delete_naming_a_literal_that_cannot_be_encoded_is_refused_naming_its_predicate() {
    let a = changesets();
    let refused = apply(
        &a,
        vec![delete(
            r#"<a> <embedding> "[0.50,-1.0]"^^ugdb:float32vector"#,
        )],
    )
    .unwrap_err();
    assert!(
        matches!(&refused.0, ChangesetErrorEnum::Literal { predicate, .. } if predicate == ex("embedding").as_str()),
        "{refused:?}"
    );
}
