/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::GraphName;
use ultragraphdb_core_catalog::CatalogErrorEnum;
use ultragraphdb_core_keys::{DEFAULT_GRAPH, Keyspace, Region};
use ultragraphdb_core_mapping::{Change, MappingErrorEnum, StatementId, StatementRef};
use ultragraphdb_store_apply::ChangesetErrorEnum;

use crate::utils::changesets::{
    NS, apply, changesets, changesets_over, declare, delete, ex, indexed, insert, insert_edge,
    objects, postings, sim_store, statement, term, tokens_of, uid,
};

fn keyspaces(regions: &std::collections::BTreeSet<Region>) -> Vec<Keyspace> {
    let mut k: Vec<Keyspace> = regions.iter().map(Region::keyspace).collect();
    k.dedup();
    k
}

#[test]
fn test_an_insert_reads_the_subjects_list_alone_and_none_of_a_new_subject() {
    let a = changesets();
    declare(&a, "tag", "with a `term` index");
    let applied = apply(&a, vec![insert(r#"<a> <tag> "x""#)]).unwrap();
    assert!(applied.read().is_empty(), "{:?}", applied.read());
    let again = apply(&a, vec![insert(r#"<a> <tag> "y""#)]).unwrap();
    let tag = uid(&a, "tag");
    assert_eq!(again.lists_read(), 1);
    assert!(
        again
            .read()
            .iter()
            .all(|r| r.keyspace() == Keyspace::Forward && r.predicate() == Some(tag))
    );
    let written = keyspaces(applied.written());
    for keyspace in [
        Keyspace::Forward,
        Keyspace::Reverse,
        Keyspace::SubjectPredicates,
        Keyspace::ObjectPredicates,
        Keyspace::ValueIndex,
        Keyspace::Statistics,
    ] {
        assert!(written.contains(&keyspace), "{keyspace:?} in {written:?}");
    }
}

#[test]
fn test_a_write_naming_new_uids_reads_none_of_their_lists() {
    let a = changesets();
    declare(
        &a,
        "friend",
        "single-valued with an exact count index, unique",
    );
    let applied = apply(&a, vec![insert("<new> <friend> <newer>")]).unwrap();
    assert!(applied.read().is_empty(), "{:?}", applied.read());
}

#[test]
fn test_a_delete_reads_the_subjects_list() {
    let a = changesets();
    apply(&a, vec![insert(r#"<a> <tag> "x""#)]).unwrap();
    let applied = apply(&a, vec![delete(r#"<a> <tag> "x""#)]).unwrap();
    let tag = uid(&a, "tag");
    assert!(
        applied
            .read()
            .iter()
            .any(|r| r.keyspace() == Keyspace::Forward && r.predicate() == Some(tag))
    );
    assert!(objects(&a, "a", "tag").is_empty());
}

#[test]
fn test_the_written_regions_name_the_predicate_and_the_graph() {
    let a = changesets();
    let applied = apply(&a, vec![insert("<a> <knows> <b>")]).unwrap();
    let knows = uid(&a, "knows");
    let has = |keyspace, predicate| {
        applied.written().iter().any(|r| {
            r.keyspace() == keyspace
                && r.namespace() == NS
                && r.graph() == Some(DEFAULT_GRAPH)
                && r.predicate() == predicate
        })
    };
    assert!(has(Keyspace::Forward, Some(knows)));
    assert!(has(Keyspace::Reverse, Some(knows)));
    assert!(has(Keyspace::SubjectPredicates, None));
    assert!(has(Keyspace::ObjectPredicates, None));
}

#[test]
fn test_one_changeset_writes_the_same_bytes_to_two_copies() {
    let history = |a: &crate::utils::changesets::TestChangesets| {
        declare(a, "name", "single-valued with a `term` index");
        apply(
            a,
            vec![
                insert(r#"<a> <name> "big apple""#),
                insert("<a> <knows> <b>"),
            ],
        )
        .unwrap();
    };
    let (one, two) = (changesets_over(sim_store()), changesets_over(sim_store()));
    history(&one);
    history(&two);
    let change = vec![
        insert(r#"<a> <name> "red apple""#),
        insert_edge("<a> <knows> <b>", &[("since", "2020")]),
        insert("<c> <knows> <a>"),
    ];
    apply(&one, change.clone()).unwrap();
    apply(&two, change).unwrap();
    assert_eq!(
        one.quads().store().contents(),
        two.quads().store().contents()
    );
}

#[test]
fn test_a_refused_change_refuses_the_whole_changeset() {
    let a = changesets();
    declare(&a, "knows", "an edge");
    let refused = apply(
        &a,
        vec![insert("<a> <knows> <b>"), insert(r#"<a> <knows> "Bob""#)],
    )
    .unwrap_err();
    assert!(
        matches!(&refused.0, ChangesetErrorEnum::Catalog(e) if matches!(e.0, CatalogErrorEnum::LiteralOnEdge { .. })),
        "{refused:?}"
    );
    assert_eq!(a.quads().uid_of(NS, ex("a").as_str()).unwrap(), None);
}

#[test]
fn test_a_non_canonical_vector_is_refused_naming_its_predicate() {
    let a = changesets();
    let refused = apply(
        &a,
        vec![insert(
            r#"<a> <embedding> "[0.50,-1.0]"^^ugdb:float32vector"#,
        )],
    )
    .unwrap_err();
    assert!(
        matches!(&refused.0, ChangesetErrorEnum::Literal { predicate, .. } if predicate == ex("embedding").as_str()),
        "{refused:?}"
    );
    assert_eq!(a.quads().uid_of(NS, ex("a").as_str()).unwrap(), None);
}

#[test]
fn test_a_property_gives_a_held_statement_an_id_and_a_record() {
    let a = changesets();
    apply(&a, vec![insert("<a> <knows> <b>")]).unwrap();
    assert_eq!(postings(&a, "a", "knows")[0].1, Vec::<u64>::new());
    let t = statement("<a> <knows> <b>");
    apply(
        &a,
        vec![Change::SetProperty {
            statement: StatementRef::Triple {
                graph: t.graph,
                subject: t.subject,
                predicate: t.predicate,
                object: t.object,
            },
            predicate: ex("since"),
            value: term("2020"),
        }],
    )
    .unwrap();
    let ids = postings(&a, "a", "knows")[0].1.clone();
    assert_eq!(ids.len(), 1);
    let record = a
        .quads()
        .statement(NS, DEFAULT_GRAPH, ids[0], a.quads().lists().latest_ts())
        .unwrap()
        .expect("a record");
    assert_eq!(record.subject(), uid(&a, "a"));
    assert_eq!(record.properties().len(), 1);
    assert_eq!(record.properties()[0].0, uid(&a, "since"));
}

#[test]
fn test_a_repeated_edge_gives_both_statements_ids() {
    let a = changesets();
    apply(&a, vec![insert("<a> <knows> <b>")]).unwrap();
    apply(&a, vec![insert_edge("<a> <knows> <b>", &[])]).unwrap();
    let held = postings(&a, "a", "knows");
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].1.len(), 2);
    // A plain insert of the held triple keeps both.
    apply(&a, vec![insert("<a> <knows> <b>")]).unwrap();
    assert_eq!(postings(&a, "a", "knows")[0].1.len(), 2);
}

#[test]
fn test_deleting_statements_removes_their_records_and_then_the_triple() {
    let a = changesets();
    apply(
        &a,
        vec![insert_edge("<a> <knows> <b>", &[("since", "2020")])],
    )
    .unwrap();
    apply(&a, vec![insert_edge("<a> <knows> <b>", &[])]).unwrap();
    let ids = postings(&a, "a", "knows")[0].1.clone();
    assert_eq!(ids.len(), 2);
    let record = |id| {
        a.quads()
            .statement(NS, DEFAULT_GRAPH, id, a.quads().lists().latest_ts())
            .unwrap()
    };
    let delete_id = |id| Change::DeleteStatement {
        graph: GraphName::DefaultGraph,
        id: StatementId::new(id),
    };
    apply(&a, vec![delete_id(ids[0])]).unwrap();
    assert!(record(ids[0]).is_none());
    assert_eq!(postings(&a, "a", "knows")[0].1, vec![ids[1]]);
    apply(&a, vec![delete_id(ids[1])]).unwrap();
    assert!(record(ids[1]).is_none());
    assert!(objects(&a, "a", "knows").is_empty());
}

#[test]
fn test_a_property_change_naming_no_statement_is_refused() {
    let a = changesets();
    apply(&a, vec![insert("<x> <p> <y>")]).unwrap();
    let t = statement("<x> <p> <z>");
    let refused = apply(
        &a,
        vec![Change::SetProperty {
            statement: StatementRef::Triple {
                graph: t.graph,
                subject: t.subject,
                predicate: t.predicate,
                object: t.object,
            },
            predicate: ex("since"),
            value: term("1"),
        }],
    )
    .unwrap_err();
    assert!(
        matches!(&refused.0, ChangesetErrorEnum::Mapping(e) if matches!(e.0, MappingErrorEnum::NoSuchStatement { .. }))
    );
}

#[test]
fn test_a_term_is_kept_as_written() {
    let a = changesets();
    declare(&a, "n", "with an `int` index");
    apply(&a, vec![insert(r#"<a> <n> "007"^^xsd:integer"#)]).unwrap();
    assert_eq!(objects(&a, "a", "n"), vec![term(r#""007"^^xsd:integer"#)]);
    let seven = &tokens_of("int", &term("7"))[0];
    assert_eq!(indexed(&a, "n", seven), vec![uid(&a, "a")]);
}

#[test]
fn test_a_delete_of_terms_the_store_does_not_hold_writes_no_dictionary_entry() {
    let a = changesets();
    apply(&a, vec![insert("<a> <knows> <b>")]).unwrap();
    let before = a.quads().store().contents().versionless().clone();
    let applied = apply(
        &a,
        vec![
            delete("<never> <knows> <b>"),
            crate::utils::changesets::delete_pattern("nobody", "knows"),
        ],
    )
    .unwrap();
    // A statement naming an IRI the store does not hold is not held: nothing is read.
    assert!(applied.read().is_empty(), "{:?}", applied.read());
    // A literal is encoded by content, so its delete reads the subject's list.
    apply(
        &a,
        vec![delete(
            r#"<a> <knows> "a long literal the store has never held""#,
        )],
    )
    .unwrap();
    assert_eq!(a.quads().store().contents().versionless(), &before);
    assert_eq!(a.quads().uid_of(NS, ex("never").as_str()).unwrap(), None);
    assert_eq!(objects(&a, "a", "knows"), vec![term("<b>")]);
}
