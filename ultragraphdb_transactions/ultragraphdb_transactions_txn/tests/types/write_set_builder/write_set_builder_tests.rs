/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::{Context, TestResult};

use oxrdf::{GraphName, Literal};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{DEFAULT_GRAPH, QuadKey};
use ultragraphdb_core_mapping::Change;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::{KeyValues, RecordKind, RecordOperation, WriteSetRecordView};
use ultragraphdb_store_apply::{DropRequest, Executed};
use ultragraphdb_transactions_txn::{ReadSet, TxnError, WriteSetBuilder};

use crate::utils::fixture::{Applier, NS, applier, changeset, ex, insert, insert_literal, store};

/// The ID the store holds for `<name>`.
fn id(applier: &Applier, name: &str) -> TestResult<SystemId> {
    applier
        .quads()
        .id_of(NS, ex(name).as_str())
        .context("the dictionary reads")?
        .context("the IRI is known")
}

/// The quad `<subject> <predicate> object` of the default graph as a record's key, from §12.2: the graph's 16 bytes
/// (the default graph's all zero), the subject's and the predicate's 16 big-endian bytes, then the object's encoding.
fn quad(subject: SystemId, predicate: SystemId, object: &[u8]) -> Vec<u8> {
    let mut key = vec![0; 16];
    key.extend_from_slice(&subject.to_be_bytes());
    key.extend_from_slice(&predicate.to_be_bytes());
    key.extend_from_slice(object);
    key
}

fn forward(applier: &Applier, subject: &str, predicate: &str) -> TestResult<Vec<u8>> {
    Ok(QuadKey::Forward {
        namespace: NS,
        graph: DEFAULT_GRAPH,
        predicate: id(applier, predicate)?,
        subject: id(applier, subject)?,
    }
    .to_bytes())
}

fn object(applier: &Applier, name: &str) -> TestResult<Vec<u8>> {
    Ok(EncodedTerm::id(id(applier, name)?).as_bytes().to_vec())
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

fn executed(applier: &Applier, changes: Vec<Change>) -> TestResult<Executed> {
    applier.execute(&changeset(changes)).context("executed")
}

/// An applier where `<a>`, `<b>`, `<c>` and `<knows>` are known.
fn known() -> TestResult<Applier> {
    let applier = applier()?;
    store(
        &applier,
        vec![insert("a", "knows", "c"), insert("b", "knows", "c")],
    )?;
    Ok(applier)
}

fn written(builder: &WriteSetBuilder<'_>) -> TestResult<Vec<u8>> {
    let mut out = Vec::new();
    let len = builder
        .write_into(&mut out, usize::MAX)
        .context("written")?;
    assert_eq!(len, out.len());
    Ok(out)
}

#[test]
fn test_the_record_reads_back_in_the_wires_layout_and_its_one_encoding() -> TestResult {
    let applier = known()?;
    let executed = executed(&applier, vec![insert("a", "knows", "b")])?;
    let session = 0x0102_0304_0506_0708_090A_0B0C_0D0E_0F10;
    let builder = WriteSetBuilder::new(&executed, session, 7, b"commit record")
        .with_principal(11)
        .with_catalog(3);
    let bytes = written(&builder)?;
    let view = WriteSetRecordView::new(&bytes).context("the wire reads it")?;
    // The wire crate's own encoder, a separate implementation of the layout, gives the same bytes again.
    assert_eq!(view.to_record().encode(), bytes);
    assert_eq!(view.kind(), RecordKind::Transaction);
    assert_eq!(view.namespace(), NS);
    assert_eq!(view.read_version(), executed.read_ts());
    assert_eq!(view.catalog(), 3);
    assert_eq!(view.session(), session.to_be_bytes());
    assert_eq!((view.request(), view.principal()), (7, 11));
    assert_eq!(view.commit_record(), b"commit record");
    assert_eq!(view.operation_count(), 0);
    assert_eq!(view.result_count(), 0);
    Ok(())
}

#[test]
fn test_the_fixed_part_is_written_from_the_layout() -> TestResult {
    let applier = known()?;
    let executed = executed(&applier, vec![insert("a", "knows", "b")])?;
    let bytes = written(&WriteSetBuilder::new(&executed, 0x0102, 0x0304, &[]).with_catalog(5))?;
    // §12.2: kind 1 and 7 zero bytes; namespace, catalog and read version little-endian; the session's 16 bytes
    // big-endian; request and principal little-endian.
    let mut fixed = vec![1, 0, 0, 0, 0, 0, 0, 0];
    fixed.extend_from_slice(&NS.to_le_bytes());
    fixed.extend_from_slice(&5u64.to_le_bytes());
    fixed.extend_from_slice(&executed.read_ts().to_le_bytes());
    fixed.extend_from_slice(&0x0102u128.to_be_bytes());
    fixed.extend_from_slice(&0x0304u64.to_le_bytes());
    fixed.extend_from_slice(&0u64.to_le_bytes());
    assert_eq!(bytes.get(..64), Some(fixed.as_slice()));
    // Section 0 starts where the table of twelve sections ends: 64 + 12 × 8 = 160.
    assert_eq!(bytes.get(64..68), Some(&160u32.to_le_bytes()[..]));
    Ok(())
}

#[test]
fn test_an_insert_records_its_quad_with_its_ids_and_names_its_statement() -> TestResult {
    let applier = known()?;
    let executed = executed(&applier, vec![insert("a", "knows", "b")])?;
    let view_bytes = written(&WriteSetBuilder::new(&executed, 1, 1, &[]))?;
    let view = WriteSetRecordView::new(&view_bytes)?;
    let (a, knows, b) = (
        id(&applier, "a")?,
        id(&applier, "knows")?,
        id(&applier, "b")?,
    );
    let b_encoded = EncodedTerm::id(b).as_bytes().to_vec();
    assert_eq!(
        entries(view.inserted()),
        vec![(quad(a, knows, &b_encoded), Some(Vec::new()))],
        "present and empty: a statement without an ID"
    );
    assert!(entries(view.removed()).is_empty());
    assert!(entries(view.statement_records()).is_empty());
    // The forward list's statement is its conflict statement, the list no point.
    assert_eq!(
        entries(view.conflict_statements()),
        vec![(
            forward(&applier, "a", "knows")?,
            Some(object(&applier, "b")?)
        )]
    );
    assert!(!keys(view.conflict_points()).contains(&forward(&applier, "a", "knows")?));
    Ok(())
}

#[test]
fn test_a_written_statement_names_its_terms_by_their_term_keys() -> TestResult {
    let applier = known()?;
    let executed = executed(&applier, vec![insert("a", "knows", "b")])?;
    let bytes = written(&WriteSetBuilder::new(&executed, 1, 1, &[]))?;
    let points = keys(WriteSetRecordView::new(&bytes)?.conflict_points());
    for name in ["a", "knows", "b"] {
        // The tag 0xFE and the ID's 16 bytes, written from the layout.
        let mut key = vec![0xFE];
        key.extend_from_slice(&id(&applier, name)?.to_be_bytes());
        assert!(points.contains(&key), "{name}: {points:?}");
        assert_eq!(ReadSet::term_key(id(&applier, name)?).to_vec(), key);
    }
    Ok(())
}

#[test]
fn test_a_literal_object_names_no_term_key() -> TestResult {
    let applier = known()?;
    let executed = executed(&applier, vec![insert_literal("a", "name", "A")])?;
    let bytes = written(&WriteSetBuilder::new(&executed, 1, 1, &[]))?;
    let view = WriteSetRecordView::new(&bytes)?;
    let term_keys: Vec<Vec<u8>> = keys(view.conflict_points())
        .into_iter()
        .filter(|k| k.first() == Some(&0xFE))
        .collect();
    // `name` is new: its ID is the execution's, not yet the dictionary's.
    let named = |n: &str| -> TestResult<Vec<u8>> {
        let iri = ex(n);
        let found = executed
            .terms()
            .written()
            .find(|(text, _)| *text == iri.as_str())
            .map(|(_, id)| id);
        let id = match found {
            Some(id) => id,
            None => id(&applier, n)?,
        };
        let mut key = vec![0xFE];
        key.extend_from_slice(&id.to_be_bytes());
        Ok(key)
    };
    let mut expected = vec![named("a")?, named("name")?];
    expected.sort();
    let mut found = term_keys;
    found.sort();
    found.dedup();
    assert_eq!(found, expected, "the subject and predicate alone");
    let inserted = keys(view.inserted());
    assert_eq!(inserted.len(), 1);
    // The object's encoding follows the graph, subject and predicate: 48 bytes in.
    let literal = inserted
        .first()
        .and_then(|k| k.get(48..))
        .context("the object")?;
    let term = EncodedTerm::from_bytes(literal).context("the object decodes")?;
    assert_eq!(
        term.as_bytes(),
        literal,
        "the rest of the key is the one literal's encoding"
    );
    Ok(())
}

#[test]
fn test_a_delete_by_pattern_records_the_statements_it_resolved_to() -> TestResult {
    let applier = known()?;
    store(
        &applier,
        vec![
            insert_literal("a", "tag", "x"),
            insert_literal("a", "tag", "y"),
        ],
    )?;
    let delete = Change::DeletePattern {
        graph: GraphName::DefaultGraph,
        subject: ex("a").into(),
        predicate: ex("tag"),
    };
    let executed = executed(&applier, vec![delete])?;
    let bytes = written(&WriteSetBuilder::new(&executed, 1, 1, &[]))?;
    let view = WriteSetRecordView::new(&bytes)?;
    let removed = entries(view.removed());
    assert_eq!(removed.len(), 2, "both statements: {removed:?}");
    let (a, tag) = (id(&applier, "a")?, id(&applier, "tag")?);
    let prefix = quad(a, tag, &[]);
    assert!(removed.iter().all(|(k, _)| k.starts_with(&prefix)));
    assert!(entries(view.inserted()).is_empty());
    Ok(())
}

#[test]
fn test_an_edge_with_properties_records_its_statement_id_and_record() -> TestResult {
    let applier = known()?;
    let edge = Change::InsertEdge {
        graph: GraphName::DefaultGraph,
        subject: ex("a").into(),
        predicate: ex("knows"),
        object: ex("b").into(),
        properties: vec![(ex("since"), Literal::new_simple_literal("2020").into())],
    };
    let executed = executed(&applier, vec![edge])?;
    let bytes = written(&WriteSetBuilder::new(&executed, 1, 1, &[]))?;
    let view = WriteSetRecordView::new(&bytes)?;
    let inserted = entries(view.inserted());
    assert_eq!(inserted.len(), 1, "{inserted:?}");
    let statement = inserted
        .first()
        .and_then(|(_, v)| v.clone())
        .context("a statement ID")?;
    assert_eq!(statement.len(), 16, "its ID's 16 bytes");
    let statement_id = SystemId::decode(&statement).context("a valid ID")?;
    // §12.2: a statement record's key is its graph's 16 bytes and the statement's ID.
    let mut record_key = vec![0; 16];
    record_key.extend_from_slice(&statement_id.to_be_bytes());
    let records = entries(view.statement_records());
    assert_eq!(records.len(), 1);
    assert_eq!(records.first().map(|(k, _)| k.clone()), Some(record_key));
    assert!(records.first().is_some_and(|(_, v)| v.is_some()));
    Ok(())
}

#[test]
fn test_a_deleted_edges_statement_record_is_recorded_as_a_removal() -> TestResult {
    let applier = known()?;
    let properties = vec![(ex("since"), Literal::new_simple_literal("2020").into())];
    store(
        &applier,
        vec![Change::InsertEdge {
            graph: GraphName::DefaultGraph,
            subject: ex("a").into(),
            predicate: ex("knows"),
            object: ex("b").into(),
            properties,
        }],
    )?;
    let delete = Change::Delete {
        graph: GraphName::DefaultGraph,
        subject: ex("a").into(),
        predicate: ex("knows"),
        object: ex("b").into(),
    };
    let executed = executed(&applier, vec![delete])?;
    let bytes = written(&WriteSetBuilder::new(&executed, 1, 1, &[]))?;
    let view = WriteSetRecordView::new(&bytes)?;
    let records = entries(view.statement_records());
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records.first().map(|(_, v)| v.clone()), Some(None));
    assert_eq!(entries(view.removed()).len(), 1);
    Ok(())
}

#[test]
fn test_a_new_iris_dictionary_entries_are_no_conflict_points() -> TestResult {
    let applier = known()?;
    let executed = executed(&applier, vec![insert("new", "knows", "c")])?;
    let bytes = written(&WriteSetBuilder::new(&executed, 1, 1, &[]))?;
    let points = keys(WriteSetRecordView::new(&bytes)?.conflict_points());
    // The dictionary keyspace's byte, 0x1B: creating an entry conflicts with nothing.
    assert!(
        points.iter().all(|k| k.first() != Some(&0x1B)),
        "{points:?}"
    );
    Ok(())
}

#[test]
fn test_a_list_changed_by_more_objects_than_the_bound_is_named_whole() -> TestResult {
    let applier = known()?;
    let executed = executed(
        &applier,
        vec![insert("a", "knows", "b"), insert("a", "knows", "x")],
    )?;
    let bytes = written(&WriteSetBuilder::new(&executed, 1, 1, &[]).with_objects_per_list(1))?;
    let view = WriteSetRecordView::new(&bytes)?;
    let list = forward(&applier, "a", "knows")?;
    assert!(keys(view.conflict_points()).contains(&list));
    assert!(keys(view.conflict_statements()).is_empty());
    // Within the bound, each object is named.
    let named = written(&WriteSetBuilder::new(&executed, 1, 1, &[]).with_objects_per_list(2))?;
    let named_view = WriteSetRecordView::new(&named)?;
    assert_eq!(
        keys(named_view.conflict_statements()),
        vec![list.clone(), list]
    );
    Ok(())
}

#[test]
fn test_a_drop_records_its_operation_and_its_ranges() -> TestResult {
    let applier = known()?;
    let drop = applier
        .prepare_drop(NS, DropRequest::Predicate(ex("knows").as_ref()))
        .context("the drop")?;
    let bytes = written(&WriteSetBuilder::new(&drop, 4, 2, &[]))?;
    let view = WriteSetRecordView::new(&bytes)?;
    assert_eq!(view.kind(), RecordKind::Transaction);
    assert_eq!(view.operation_count(), 1);
    assert_eq!(
        view.operation(0),
        Some(RecordOperation::DropPredicate(id(&applier, "knows")?))
    );
    assert_eq!(view.read_version(), 0, "a Drop's read version is 0");
    let ranges: Vec<(Vec<u8>, Vec<u8>)> = view
        .conflict_ranges()
        .iter()
        .map(|e| (e.key().to_vec(), e.value().unwrap_or_default().to_vec()))
        .collect();
    assert_eq!(ranges, drop.drop_ranges().to_vec());
    assert!(entries(view.inserted()).is_empty());
    Ok(())
}

#[test]
fn test_each_drop_names_its_operation() -> TestResult {
    let applier = known()?;
    store(
        &applier,
        vec![Change::Insert {
            graph: GraphName::NamedNode(ex("g")),
            subject: ex("a").into(),
            predicate: ex("knows"),
            object: ex("c").into(),
        }],
    )?;
    let (knows, g) = (ex("knows"), ex("g"));
    let cases = [
        (
            DropRequest::Predicate(knows.as_ref()),
            RecordOperation::DropPredicate(id(&applier, "knows")?),
        ),
        (
            DropRequest::Graph(g.as_ref().into()),
            RecordOperation::DropGraph(id(&applier, "g")?),
        ),
        (DropRequest::Data, RecordOperation::DropNamespaceData),
        (DropRequest::Namespace, RecordOperation::DropNamespace),
        (DropRequest::All, RecordOperation::DropAll),
    ];
    for (request, operation) in cases {
        let drop = applier.prepare_drop(NS, request).context("the drop")?;
        let bytes = written(&WriteSetBuilder::new(&drop, 9, 3, &[]))?;
        let view = WriteSetRecordView::new(&bytes)?;
        assert_eq!(view.operation(0), Some(operation), "{operation:?}");
        assert_eq!((view.session(), view.request()), (9u128.to_be_bytes(), 3));
    }
    Ok(())
}

#[test]
fn test_a_record_at_its_limit_is_written_and_one_byte_over_is_refused_leaving_the_buffer_empty()
-> TestResult {
    let applier = known()?;
    let executed = executed(&applier, vec![insert("a", "knows", "b")])?;
    let builder = WriteSetBuilder::new(&executed, 1, 1, b"r");
    let len = written(&builder)?.len();
    let mut out = Vec::with_capacity(8_192);
    assert_eq!(builder.write_into(&mut out, len), Ok(len));
    assert_eq!(out.len(), len);
    let capacity = out.capacity();
    assert_eq!(
        builder.write_into(&mut out, len - 1),
        Err(TxnError::write_set_too_large(len, len - 1))
    );
    assert!(out.is_empty());
    assert_eq!(out.capacity(), capacity);
    Ok(())
}

#[test]
fn test_a_built_record_encodes_as_its_writer_writes_it() -> TestResult {
    let applier = known()?;
    let executed = executed(&applier, vec![insert("a", "knows", "b")])?;
    let builder = WriteSetBuilder::new(&executed, 5, 6, b"x").with_principal(2);
    let record = builder.build()?;
    let mut out = vec![0xAA; 3];
    let len = WriteSetBuilder::encode_into(&record, &mut out, usize::MAX)?;
    assert_eq!(out, record.encode());
    assert_eq!(len, out.len());
    assert_eq!(out, written(&builder)?);
    Ok(())
}

#[test]
fn test_a_reused_buffer_is_cleared() -> TestResult {
    let applier = known()?;
    let big = executed(
        &applier,
        (0..20)
            .map(|i| insert(&format!("s{i}"), "knows", "b"))
            .collect(),
    )?;
    let small = executed(&applier, vec![insert("a", "knows", "b")])?;
    let mut out = Vec::new();
    let big_len = WriteSetBuilder::new(&big, 1, 1, &[]).write_into(&mut out, usize::MAX)?;
    let small_len = WriteSetBuilder::new(&small, 1, 1, &[]).write_into(&mut out, usize::MAX)?;
    assert!(small_len < big_len);
    assert_eq!(out.len(), small_len);
    assert_eq!(out, written(&WriteSetBuilder::new(&small, 1, 1, &[]))?);
    Ok(())
}

/// A record encoded past its limit is refused naming its size and the limit, the buffer left empty; at the limit it is
/// written.
#[test]
fn test_an_encoding_past_its_limit_is_refused_leaving_the_buffer_empty() -> TestResult {
    let applier = known()?;
    let executed = executed(&applier, vec![insert("a", "knows", "b")])?;
    let record = WriteSetBuilder::new(&executed, 1, 1, &[]).build()?;
    let len = record.encode().len();
    let mut out = vec![0xAA; 3];
    assert_eq!(
        WriteSetBuilder::encode_into(&record, &mut out, len - 1),
        Err(TxnError::write_set_too_large(len, len - 1))
    );
    assert!(out.is_empty());
    assert_eq!(
        WriteSetBuilder::encode_into(&record, &mut out, len),
        Ok(len)
    );
    assert_eq!(out, record.encode());
    Ok(())
}

/// A statement written in a named graph names the graph's term too, by its term key; the default graph names none.
#[test]
fn test_a_statement_in_a_named_graph_names_its_graphs_term_key() -> TestResult {
    let applier = known()?;
    let in_g = |subject: &str| Change::Insert {
        graph: GraphName::NamedNode(ex("g")),
        subject: ex(subject).into(),
        predicate: ex("knows"),
        object: ex("c").into(),
    };
    store(&applier, vec![in_g("b")])?;
    let executed = executed(&applier, vec![in_g("a")])?;
    let bytes = written(&WriteSetBuilder::new(&executed, 1, 1, &[]))?;
    let points = keys(WriteSetRecordView::new(&bytes)?.conflict_points());
    let mut term_keys: Vec<Vec<u8>> = points
        .into_iter()
        .filter(|k| k.first() == Some(&ReadSet::TERM_TAG))
        .collect();
    term_keys.sort();
    let mut expected = ["a", "knows", "c", "g"]
        .iter()
        .map(|n| id(&applier, n).map(|id| ReadSet::term_key(id).to_vec()))
        .collect::<TestResult<Vec<_>>>()?;
    expected.sort();
    assert_eq!(term_keys, expected, "subject, predicate, object and graph");
    Ok(())
}
