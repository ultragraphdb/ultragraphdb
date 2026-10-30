/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeSet;

use oxrdf::{GraphName, Literal};
use ultragraphdb_core_keys::QuadKey;
use ultragraphdb_core_mapping::Change;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::{KeyValues, RecordOperation, WriteSetRecordView};
use ultragraphdb_store_apply::Executed;
use ultragraphdb_store_quads::{iri_key, uid_key, uid_value};
use ultragraphdb_transactions_txn::{TxnError, WriteSetBuilder};

use crate::utils::fixture::{Applier, NS, applier, changeset, commit, ex, insert};

/// The UID the store holds for `<name>`.
fn uid(applier: &Applier, name: &str) -> u64 {
    applier
        .quads()
        .uid_of(NS, ex(name).as_str())
        .expect("the dictionary reads")
        .expect("the IRI is known")
}

fn forward(applier: &Applier, subject: &str, predicate: &str) -> Vec<u8> {
    QuadKey::Forward {
        namespace: NS,
        graph: 0,
        predicate: uid(applier, predicate),
        subject: uid(applier, subject),
    }
    .to_bytes()
}

fn reverse(applier: &Applier, predicate: &str, object: &str) -> Vec<u8> {
    QuadKey::Reverse {
        namespace: NS,
        graph: 0,
        predicate: uid(applier, predicate),
        object: EncodedTerm::uid(uid(applier, object)),
    }
    .to_bytes()
}

fn object(applier: &Applier, name: &str) -> Vec<u8> {
    EncodedTerm::uid(uid(applier, name)).as_bytes().to_vec()
}

fn keys(values: KeyValues<'_>) -> Vec<Vec<u8>> {
    values.iter().map(|e| e.key().to_vec()).collect()
}

fn entries(values: KeyValues<'_>) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
    values
        .iter()
        .map(|e| (e.key().to_vec(), e.value().map(<[u8]>::to_vec)))
        .collect()
}

/// `changes` executed on `applier`, where `<a>`, `<b>`, `<c>`, `<knows>` are known and `knows` has its default
/// catalog entry.
fn executed(applier: &Applier, changes: Vec<Change>) -> Executed {
    applier.execute(&changeset(changes)).expect("executed")
}

fn known() -> Applier {
    let applier = applier();
    commit(
        &applier,
        vec![insert("a", "knows", "c"), insert("b", "knows", "c")],
    );
    applier
}

fn written(builder: &WriteSetBuilder<'_>) -> Vec<u8> {
    let mut out = Vec::new();
    let len = builder.write_into(&mut out, usize::MAX).expect("written");
    assert_eq!(len, out.len());
    out
}

#[test]
fn test_the_record_is_in_the_wires_layout_and_its_one_encoding() {
    let applier = known();
    let executed = executed(&applier, vec![insert("a", "knows", "b")]);
    let builder = WriteSetBuilder::new(&executed, 9, 7, b"commit record");
    let bytes = written(&builder);
    // The wire crate's own encoder, a separate implementation of the layout, gives the same bytes again.
    let view = WriteSetRecordView::new(&bytes).expect("the wire reads it");
    assert_eq!(view.to_record().encode(), bytes);
    assert_eq!(builder.len(), bytes.len());
    assert!(!builder.is_empty());
    assert_eq!(view.namespace(), NS);
    assert_eq!(view.read_version(), executed.read_ts());
    assert_eq!((view.session(), view.request()), (9, 7));
    assert_eq!(view.commit_record(), b"commit record");
}

#[test]
fn test_the_fixed_part_is_written_little_endian_first() {
    // The layout table of `WriteSetRecord`: namespace, read version, session, request, each u64 little-endian.
    let applier = known();
    let executed = executed(&applier, vec![insert("a", "knows", "b")]);
    let bytes = written(&WriteSetBuilder::new(&executed, 0x0102, 0x0304, &[]));
    let fixed: Vec<u8> = [NS, executed.read_ts(), 0x0102, 0x0304]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    assert_eq!(&bytes[..32], fixed.as_slice());
    // Section 0 starts where the table of ten sections ends: 32 + 10 × 8.
    assert_eq!(u32::from_le_bytes(bytes[32..36].try_into().unwrap()), 120);
}

/// A worker's record holds no results; the commit step's record of a transaction with operations holds each one in
/// section 9, in the order asked, which the wire's view reads back (decided 2026-10-28).
#[test]
fn test_results_are_written_into_section_nine_in_order() {
    let applier = known();
    let executed = executed(&applier, vec![insert("a", "knows", "b")]);
    let none = written(&WriteSetBuilder::new(&executed, 1, 1, &[]));
    assert_eq!(WriteSetRecordView::new(&none).unwrap().result_count(), 0);
    let results = [(41, true), (7, false)];
    let builder = WriteSetBuilder::new(&executed, 1, 1, &[]).with_results(&results);
    let bytes = written(&builder);
    assert_eq!(bytes.len(), builder.len());
    assert_eq!(bytes.len(), none.len() + 32);
    let view = WriteSetRecordView::new(&bytes).unwrap();
    let read: Vec<(u64, bool)> = (0..view.result_count())
        .filter_map(|i| view.result(i))
        .map(|r| (r.uid(), r.created()))
        .collect();
    assert_eq!(read, results);
}

/// A drop's record names its operation in section 10, the predicate's UID; a build's end names its record; any other
/// execution's record has none (decided 2026-10-28).
#[test]
fn test_a_drop_and_a_builds_end_name_their_operation_in_section_ten() {
    let applier = known();
    let plain = executed(&applier, vec![insert("a", "knows", "b")]);
    let bytes = written(&WriteSetBuilder::new(&plain, 1, 1, &[]));
    assert_eq!(WriteSetRecordView::new(&bytes).unwrap().operation(), None);
    let drop = applier
        .prepare_drop_predicate(NS, ex("knows").as_ref())
        .unwrap();
    let builder = WriteSetBuilder::new(&drop, 0, 0, &[]);
    let bytes = written(&builder);
    assert_eq!(bytes.len(), builder.len());
    assert_eq!(
        WriteSetRecordView::new(&bytes).unwrap().operation(),
        Some(RecordOperation::DropPredicate(uid(&applier, "knows")))
    );
    for (end, operation) in [
        (
            ultragraphdb_store_apply::BuildEnd::Swap(12),
            RecordOperation::SwapBuild(12),
        ),
        (
            ultragraphdb_store_apply::BuildEnd::Undo(14),
            RecordOperation::UndoBuild(14),
        ),
    ] {
        let ending = applier.prepare_build_end(end).unwrap();
        let builder = WriteSetBuilder::new(&ending, 0, 0, &[]);
        let bytes = written(&builder);
        assert_eq!(bytes.len(), builder.len());
        assert_eq!(
            WriteSetRecordView::new(&bytes).unwrap().operation(),
            Some(operation)
        );
    }
}

#[test]
fn test_an_insert_writes_its_lists_deltas_and_names_its_forward_statement() {
    let applier = known();
    let executed = executed(&applier, vec![insert("a", "knows", "b")]);
    let bytes = written(&WriteSetBuilder::new(&executed, 1, 1, &[]));
    let view = WriteSetRecordView::new(&bytes).unwrap();
    let mut expected_deltas: Vec<Vec<u8>> = executed
        .writes()
        .deltas()
        .iter()
        .map(|(k, _)| k.clone())
        .collect();
    let statistics = QuadKey::Statistics {
        namespace: NS,
        graph: 0,
        predicate: uid(&applier, "knows"),
    }
    .to_bytes();
    expected_deltas.push(statistics.clone());
    let mut deltas = keys(view.list_deltas());
    deltas.sort();
    expected_deltas.sort();
    assert_eq!(deltas, expected_deltas);
    assert!(deltas.contains(&forward(&applier, "a", "knows")));
    assert!(deltas.contains(&reverse(&applier, "knows", "b")));
    // Each list's delta in the posting format, as the posting list encodes itself.
    for (key, delta) in executed.writes().deltas() {
        let found = view
            .list_deltas()
            .iter()
            .find(|e| e.key() == key.as_slice())
            .expect("the delta is there");
        assert_eq!(found.value(), Some(delta.encode().as_slice()));
    }
    // The forward list changed by one object is named by that object; every other write is a point.
    let statements = entries(view.conflict_statements());
    assert_eq!(
        statements,
        vec![(forward(&applier, "a", "knows"), Some(object(&applier, "b")))]
    );
    let points = keys(view.conflict_points());
    assert!(!points.contains(&forward(&applier, "a", "knows")));
    assert!(points.contains(&reverse(&applier, "knows", "b")));
    assert!(points.contains(&statistics));
    assert_eq!(points.len(), deltas.len() - 1);
    assert_eq!(view.conflict_ranges().len(), 0);
    assert_eq!(view.dictionary().len(), 0);
    assert_eq!(view.statements().len(), 0);
    assert_eq!(view.catalog().len(), 0);
    assert_eq!(view.versionless().len(), 0);
}

#[test]
fn test_a_list_changed_by_more_objects_than_the_bound_is_named_whole() {
    let applier = known();
    commit(&applier, vec![insert("d", "knows", "c")]);
    let executed = executed(
        &applier,
        vec![insert("a", "knows", "b"), insert("a", "knows", "d")],
    );
    let key = forward(&applier, "a", "knows");
    let at = |bound: usize| {
        let bytes =
            written(&WriteSetBuilder::new(&executed, 1, 1, &[]).with_objects_per_list(bound));
        let view = WriteSetRecordView::new(&bytes).unwrap();
        let statements: BTreeSet<_> = entries(view.conflict_statements()).into_iter().collect();
        (keys(view.conflict_points()).contains(&key), statements)
    };
    let (point, statements) = at(2);
    assert!(
        !point,
        "two objects within a bound of two: named one by one"
    );
    let expected: BTreeSet<_> = [
        (key.clone(), Some(object(&applier, "b"))),
        (key.clone(), Some(object(&applier, "d"))),
    ]
    .into_iter()
    .collect();
    assert_eq!(statements, expected);
    let (point, statements) = at(1);
    assert!(point, "two objects past a bound of one: the list whole");
    assert!(statements.is_empty());
    let (point, _) = at(0);
    assert!(point);
}

#[test]
fn test_a_new_iri_writes_its_dictionary_entries_as_points() {
    let applier = known();
    let executed = executed(&applier, vec![insert("n", "knows", "b")]);
    let bytes = written(&WriteSetBuilder::new(&executed, 1, 1, &[]));
    let view = WriteSetRecordView::new(&bytes).unwrap();
    let (iri, uid) = executed.terms().created().next().expect("n is created");
    assert_eq!(iri, ex("n").as_str());
    let mut dictionary = entries(view.dictionary());
    dictionary.sort();
    let mut expected = vec![
        (iri_key(NS, iri), Some(uid.to_be_bytes().to_vec())),
        (uid_key(NS, uid), Some(uid_value(iri, false))),
    ];
    expected.sort();
    assert_eq!(dictionary, expected);
    let points = keys(view.conflict_points());
    assert!(points.contains(&iri_key(NS, iri)));
    assert!(points.contains(&uid_key(NS, uid)));
    assert_eq!(view.versionless().len(), 0);
}

/// A predicate's implicit default entry is an insert-if-absent on its catalog key, not a catalog change (decided
/// 2026-10-27): the record carries the entry and names no conflict point for it, so concurrent first writes of one
/// predicate both commit.
#[test]
fn test_a_predicates_first_write_writes_its_default_entry_without_a_point() {
    let applier = known();
    let executed = executed(&applier, vec![insert("a", "likes", "b")]);
    let bytes = written(&WriteSetBuilder::new(&executed, 1, 1, &[]));
    let view = WriteSetRecordView::new(&bytes).unwrap();
    let catalog = keys(view.catalog());
    assert_eq!(catalog.len(), 1, "the default entry of likes");
    // A catalog key: the catalog keyspace byte, then the namespace big-endian.
    let mut prefix = vec![0x01];
    prefix.extend_from_slice(&NS.to_be_bytes());
    assert!(catalog[0].starts_with(&prefix));
    assert!(!keys(view.conflict_points()).contains(&catalog[0]));
    assert!(view.catalog().iter().all(|e| e.value().is_some()));
}

/// A declaration remains a catalog change: its entry is a conflict point, even when the changeset first wrote the
/// predicate under its default entry.
#[test]
fn test_a_declarations_catalog_entry_is_a_point() {
    let applier = known();
    let declared = ultragraphdb_core_mapping::PredicateDeclaration::new(ex("likes"));
    let executed = executed(
        &applier,
        vec![
            insert("a", "likes", "b"),
            Change::Declare(ultragraphdb_core_mapping::Declaration::Predicate(declared)),
        ],
    );
    let bytes = written(&WriteSetBuilder::new(&executed, 1, 1, &[]));
    let view = WriteSetRecordView::new(&bytes).unwrap();
    let catalog = keys(view.catalog());
    assert_eq!(catalog.len(), 1, "the declaration of likes");
    assert!(keys(view.conflict_points()).contains(&catalog[0]));
}

#[test]
fn test_a_deleted_edges_statement_record_is_written_as_a_removal() {
    let applier = known();
    let properties = vec![(ex("since"), Literal::new_simple_literal("2020").into())];
    let (subject, object) = (ex("a"), ex("b"));
    commit(
        &applier,
        vec![Change::InsertEdge {
            graph: GraphName::DefaultGraph,
            subject: subject.clone().into(),
            predicate: ex("knows"),
            object: object.clone().into(),
            properties,
        }],
    );
    let delete = Change::Delete {
        graph: GraphName::DefaultGraph,
        subject: subject.into(),
        predicate: ex("knows"),
        object: object.into(),
    };
    let executed = executed(&applier, vec![delete]);
    let bytes = written(&WriteSetBuilder::new(&executed, 1, 1, &[]));
    let view = WriteSetRecordView::new(&bytes).unwrap();
    let records = entries(view.statements());
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].1, None, "a removal, not an empty value");
}

#[test]
fn test_an_edges_statement_record_is_written_with_its_key_a_point() {
    let applier = known();
    let edge = Change::InsertEdge {
        graph: GraphName::DefaultGraph,
        subject: ex("a").into(),
        predicate: ex("knows"),
        object: ex("b").into(),
        properties: vec![(ex("since"), Literal::new_simple_literal("2020").into())],
    };
    let executed = executed(&applier, vec![edge]);
    let bytes = written(&WriteSetBuilder::new(&executed, 1, 1, &[]));
    let view = WriteSetRecordView::new(&bytes).unwrap();
    let records = keys(view.statements());
    assert_eq!(records.len(), 1);
    // A statement record's key: the statements keyspace byte 0x17, then the namespace.
    let mut prefix = vec![0x17];
    prefix.extend_from_slice(&NS.to_be_bytes());
    assert!(records[0].starts_with(&prefix));
    assert!(keys(view.conflict_points()).contains(&records[0]));
    // The record is not a catalog entry, and the catalog entry of `since` is not a record.
    assert!(!keys(view.catalog()).contains(&records[0]));
}

#[test]
fn test_a_record_at_its_limit_is_written_and_one_byte_over_is_refused_leaving_the_buffer_empty() {
    let applier = known();
    let executed = executed(&applier, vec![insert("a", "knows", "b")]);
    let builder = WriteSetBuilder::new(&executed, 1, 1, b"r");
    let len = builder.len();
    let mut out = Vec::with_capacity(8_192);
    assert_eq!(builder.write_into(&mut out, len), Ok(len));
    assert_eq!(out.len(), len);
    let capacity = out.capacity();
    assert_eq!(
        builder.write_into(&mut out, len - 1),
        Err(TxnError::WriteSetTooLarge(len, len - 1))
    );
    assert!(out.is_empty());
    assert_eq!(out.capacity(), capacity);
}

#[test]
fn test_a_reused_buffer_is_cleared_and_keeps_its_capacity() {
    let applier = known();
    let big = executed(
        &applier,
        vec![insert("a", "knows", "b"), insert("n", "likes", "m")],
    );
    let small = executed(&applier, vec![insert("b", "knows", "a")]);
    let mut out = Vec::new();
    WriteSetBuilder::new(&big, 1, 1, &[])
        .write_into(&mut out, usize::MAX)
        .unwrap();
    let capacity = out.capacity();
    let small_builder = WriteSetBuilder::new(&small, 2, 2, &[]);
    small_builder.write_into(&mut out, usize::MAX).unwrap();
    assert_eq!(
        out,
        written(&small_builder),
        "nothing of the earlier record is left"
    );
    assert_eq!(out.capacity(), capacity);
    WriteSetBuilder::new(&big, 1, 1, &[])
        .write_into(&mut out, usize::MAX)
        .unwrap();
    assert_eq!(
        out.capacity(),
        capacity,
        "a record that fitted fits again without growing"
    );
}

#[test]
fn test_the_written_size_reserves_once() {
    let applier = known();
    let executed = executed(&applier, vec![insert("a", "knows", "b")]);
    let builder = WriteSetBuilder::new(&executed, 1, 1, &[]);
    let mut out = Vec::new();
    builder.write_into(&mut out, usize::MAX).unwrap();
    assert_eq!(
        out.capacity(),
        builder.len(),
        "one reservation of the exact size"
    );
}
