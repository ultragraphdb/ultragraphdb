/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphName, GraphNameRef};
use ultragraphdb_core_catalog::CatalogErrorEnum;
use ultragraphdb_core_keys::{DEFAULT_GRAPH, Keyspace, Region};
use ultragraphdb_core_mapping::{Change, MappingErrorEnum, StatementId, StatementRef};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_core_tok::TokErrorEnum;
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_apply::ChangesetErrorEnum;
use ultragraphdb_store_hnsw::HnswErrorEnum;
use ultragraphdb_store_posting::{PostingError, PostingErrorEnum};
use ultragraphdb_store_sim::Fault;
use ultragraphdb_traits_index::VectorIndexOptions;

use crate::utils::changesets::{
    NS, apply, changesets, changesets_over, declare, delete, ex, index, indexed, insert,
    insert_edge, int_token, objects, postings, sim_store, statement, term, uid, write,
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
    // An undeclared predicate has no value or count index, so its write touches neither keyspace.
    let touched = keyspaces(applied.written());
    assert!(!touched.contains(&Keyspace::ValueIndex), "{touched:?}");
    assert!(!touched.contains(&Keyspace::CountIndex), "{touched:?}");
    // With a count index declared, the write touches the count keyspace, and still no value index.
    declare(&a, "likes", "with an exact count index");
    let counted = apply(&a, vec![insert("<a> <likes> <b>")]).unwrap();
    let touched = keyspaces(counted.written());
    assert!(touched.contains(&Keyspace::CountIndex), "{touched:?}");
    assert!(!touched.contains(&Keyspace::ValueIndex), "{touched:?}");
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
    assert_eq!(
        refused.to_string(),
        "Predicate <http://ex.org/knows> is declared an edge and cannot hold the literal \"Bob\""
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
    // "0.50,-1.0" is one element, split at ", " only, and no 32-bit float.
    assert_eq!(
        refused.to_string(),
        "Predicate <http://ex.org/embedding> cannot hold the literal: Invalid vector literal \"[0.50,-1.0]\": an element is no 32-bit float"
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
        .statement(NS, DEFAULT_GRAPH, ids[0], a.quads().lists().applied_ts())
        .unwrap()
        .expect("a record");
    assert_eq!(record.subject(), uid(&a, "a"));
    assert_eq!(record.predicate(), uid(&a, "knows"));
    assert_eq!(record.object(), &EncodedTerm::uid(uid(&a, "b")));
    let [(property, value)] = record.properties() else {
        panic!("one property: {:?}", record.properties())
    };
    assert_eq!(*property, uid(&a, "since"));
    assert_eq!(a.quads().term_of(NS, value).unwrap(), term("2020"));
}

#[test]
fn test_a_repeated_edge_gives_both_statements_ids() {
    let a = changesets();
    apply(&a, vec![insert("<a> <knows> <b>")]).unwrap();
    apply(&a, vec![insert_edge("<a> <knows> <b>", &[])]).unwrap();
    let held = postings(&a, "a", "knows");
    assert_eq!(held.len(), 1);
    let ids = held[0].1.clone();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
    for id in &ids {
        let record = a
            .quads()
            .statement(NS, DEFAULT_GRAPH, *id, a.quads().lists().applied_ts())
            .unwrap()
            .expect("a record");
        assert_eq!(
            (record.subject(), record.predicate(), record.object()),
            (
                uid(&a, "a"),
                uid(&a, "knows"),
                &EncodedTerm::uid(uid(&a, "b"))
            )
        );
        assert!(record.properties().is_empty());
    }
    // A plain insert of the held triple keeps both.
    apply(&a, vec![insert("<a> <knows> <b>")]).unwrap();
    assert_eq!(postings(&a, "a", "knows")[0].1, ids);
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
            .statement(NS, DEFAULT_GRAPH, id, a.quads().lists().applied_ts())
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
    assert_eq!(
        refused.to_string(),
        "No statement <http://ex.org/x> <http://ex.org/p> <http://ex.org/z> is held"
    );
}

#[test]
fn test_a_term_is_kept_as_written() {
    let a = changesets();
    declare(&a, "n", "with an `int` index");
    apply(&a, vec![insert(r#"<a> <n> "007"^^xsd:integer"#)]).unwrap();
    assert_eq!(objects(&a, "a", "n"), vec![term(r#""007"^^xsd:integer"#)]);
    assert_eq!(indexed(&a, "n", &int_token(7)), vec![uid(&a, "a")]);
    // The lexical form is not indexed as a string: no int key of 007 read as 0 or 700.
    assert!(indexed(&a, "n", &int_token(0)).is_empty());
    assert!(indexed(&a, "n", &int_token(700)).is_empty());
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

/// A batch the store fails to persist refuses the changeset with the store's error, and nothing of it is held; the
/// store is then poisoned, and the next changeset is refused with that. The texts are the simulated store's
/// (`ultragraphdb_store_sim`, `Fault::FailNextBatch`) and `StoreError::Poisoned`'s.
#[test]
fn test_a_batch_the_store_fails_refuses_the_changeset_with_the_store_s_error() {
    let a = changesets();
    apply(&a, vec![insert("<a> <knows> <b>")]).unwrap();
    let watermark = a.quads().lists().applied_ts();
    a.quads().store().inject(Fault::FailNextBatch);
    let refused = apply(&a, vec![insert("<a> <knows> <c>")]).unwrap_err();
    // The posting lists' commit writes the batch, so the store's error reaches the changeset through theirs.
    assert_eq!(
        refused.0,
        ChangesetErrorEnum::Posting(PostingError(PostingErrorEnum::Store(
            StoreError::WriteFailed("simulated failure during the batch's persist".into())
        ))),
        "{refused:?}"
    );
    assert_eq!(a.quads().lists().applied_ts(), watermark);
    let poisoned = apply(&a, vec![insert("<a> <knows> <d>")]).unwrap_err();
    assert_eq!(
        poisoned.0,
        ChangesetErrorEnum::Posting(PostingError(PostingErrorEnum::Store(StoreError(
            StoreErrorEnum::Poisoned
        )))),
        "{poisoned:?}"
    );
}

/// A drop whose read of the store fails is refused with the store's own error, which no other crate wraps: a drop
/// scans the store's snapshot directly. The text is the simulated store's (`Fault::FailNextScanAfter`). Nothing is
/// dropped.
#[test]
fn test_a_drop_whose_scan_fails_is_refused_with_the_store_s_error() {
    let a = changesets();
    apply(&a, vec![insert("<a> <knows> <b>")]).unwrap();
    a.quads().store().inject(Fault::FailNextScanAfter(0));
    let refused = a.drop_predicate(NS, ex("knows").as_ref()).unwrap_err();
    assert!(
        matches!(
            &refused.0,
            ChangesetErrorEnum::Store(StoreError(StoreErrorEnum::ReadFailed { reason, .. }))
                if reason == "simulated read failure partway through a scan"
        ),
        "{refused:?}"
    );
    assert_eq!(objects(&a, "a", "knows"), vec![term("<b>")]);
}

/// A geometry the geo index cannot cover refuses the write with the tokenizer's error: Dgraph's `indexCells`
/// (`types/s2index.go`) indexes points, polygons and multipolygons, and refuses any other geometry with
/// `Cannot index geometry of type %T`, here `*geom.LineString`. Nothing of the changeset is held.
#[test]
fn test_a_geometry_the_geo_index_cannot_cover_is_refused_with_the_tokenizer_s_error() {
    let a = changesets();
    declare(&a, "where", "with a `geo` index");
    let line = r#"<a> <where> "{\"type\": \"LineString\", \"coordinates\": [[8.63, 50.12], [8.64, 50.13]]}"^^geo:geoJSONLiteral"#;
    let refused = apply(&a, vec![insert(line)]).unwrap_err();
    assert!(
        matches!(&refused.0, ChangesetErrorEnum::Tok(t) if matches!(t.0, TokErrorEnum::Geo(_))),
        "{refused:?}"
    );
    assert_eq!(
        refused.to_string(),
        "Cannot index geometry of type *geom.LineString"
    );
    assert_eq!(a.quads().uid_of(NS, ex("a").as_str()).unwrap(), None);
    // The same predicate without the index holds the line.
    declare(&a, "where", "");
    apply(&a, vec![insert(line)]).unwrap();
    assert_eq!(objects(&a, "a", "where").len(), 1);
}

/// A search whose query is not of the index's dimension is refused with the index's error: Dgraph's
/// `applyDistanceFunction` (`tok/hnsw/helper.go`) refuses vectors of different lengths, naming the metric, euclidean
/// by default.
#[test]
fn test_a_search_with_a_query_of_another_dimension_is_refused_with_the_index_s_error() {
    let a = changesets();
    declare(
        &a,
        "emb",
        "single-valued with a vector index of dimension 2",
    );
    write(
        &a,
        &[
            "<a> <emb> \"[1, 0]\"^^ugdb:float32vector",
            "<b> <emb> \"[0, 1]\"^^ugdb:float32vector",
        ],
    );
    index(&a, "emb");
    let search = |query: &[f32]| {
        a.search_vectors(
            NS,
            GraphNameRef::DefaultGraph,
            ex("emb").as_ref(),
            query,
            2,
            &VectorIndexOptions::default(),
            a.quads().lists().applied_ts(),
        )
    };
    // A query of the index's dimension finds the nearer first: [1, 0] is at 0 from `a`, at sqrt(2) from `b`.
    assert_eq!(
        search(&[1.0, 0.0]).unwrap(),
        vec![uid(&a, "a"), uid(&a, "b")]
    );
    let refused = search(&[1.0, 0.0, 0.0]).unwrap_err();
    assert_eq!(
        refused.0,
        ChangesetErrorEnum::Hnsw(ultragraphdb_store_hnsw::HnswError(
            HnswErrorEnum::DifferentLengths("euclidean distance")
        )),
        "{refused:?}"
    );
    assert_eq!(
        refused.to_string(),
        "can not compute euclidean distance on vectors of different lengths"
    );
}
