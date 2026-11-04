/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU64;

use ultragraphdb_core_ids::{Bucket, BucketGroup, Sequence, SystemId, TypePrefix};
use ultragraphdb_protocol_wire::{
    OperationResult, RecordKind, RecordOperation, RecordRegistration, RequestTerm, WriteSetRecord,
    WriteSetRecordView,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::write_set_record_view::write_set_record_view_tests::golden_records;

/// The system ID of `prefix`, `sequence` and `bucket`, built through `core_ids`, as generate_bodies.py's `sid` lays
/// it out.
pub fn sid(prefix: u16, sequence: u64, bucket: u32) -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::new(prefix)?,
        Bucket::new(bucket)?,
        Sequence::new(sequence)?,
    ))
}

/// The registration of `entity` for `group` at `generation`.
pub fn registration(
    entity: SystemId,
    group: u32,
    generation: u64,
) -> TestResult<RecordRegistration> {
    Ok(RecordRegistration::new(
        entity,
        BucketGroup::new(group)?,
        NonZeroU64::new(generation).context("a generation from 1")?,
    ))
}

/// The session of the golden records: the bytes 0x11 to 0x20.
pub const SESSION: [u8; 16] = [
    0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F, 0x20,
];

/// The golden Round's first version: epoch 7, sequence 100.
pub const FIRST_VERSION: u64 = (7 << 22) | 100;

/// Record 0 of the golden Round, a transaction, as generate_bodies.py lays it out.
pub fn record0() -> TestResult<WriteSetRecord> {
    let mut record = WriteSetRecord::transaction(3);
    record.set_catalog(11);
    record.set_read_version((7 << 22) | 50);
    record.set_session(SESSION, 7);
    record.set_principal(9);
    record.push_inserted(b"quad-a".to_vec(), Some(sid(2, 5, 0x2A)?.to_be_bytes()));
    record.push_inserted(b"quad-b".to_vec(), None);
    record.push_removed(b"quad-c".to_vec(), Some(sid(2, 6, 0x2A)?.to_be_bytes()));
    record.push_statement_record(b"g+stmt-5".to_vec(), Some(b"record-5".to_vec()));
    record.push_statement_record(b"g+stmt-6".to_vec(), None);
    record.push_registration(registration(sid(1, 9, 0x40)?, 1, 3)?);
    record.set_commit_record((0..4_000u32).map(|i| ((i * 31 + 7) % 251) as u8).collect());
    record.push_conflict_point(b"\x10key-a".to_vec());
    record.push_conflict_statement(b"\x10list-a".to_vec(), b"\x01obj".to_vec());
    record.push_conflict_range(b"\x30a".to_vec(), b"\x30b".to_vec());
    record.push_node_result("http://ex.org/x", true);
    record.push_result(OperationResult::counter(-5));
    record.push_result(OperationResult::compare_and_set());
    Ok(record)
}

/// Record 1: a registration the partition accepted as the entity's home.
pub fn record1() -> TestResult<WriteSetRecord> {
    Ok(WriteSetRecord::registration(
        3,
        registration(sid(1, 12, 0x80)?, 2, 1)?,
    ))
}

/// Record 2: the start of the build of catalog version 12 in namespace 4.
pub fn record2() -> WriteSetRecord {
    let mut record = WriteSetRecord::build(4, RecordOperation::CatalogTakesEffect(12));
    record.set_catalog(12);
    record
}

/// Record 3: a Drop of a predicate, through session `SESSION`'s request 8.
pub fn record3() -> TestResult<WriteSetRecord> {
    let mut record = WriteSetRecord::transaction(3);
    record.set_catalog(11);
    record.set_session(SESSION, 8);
    record.set_principal(9);
    record.push_operation(RecordOperation::DropPredicate(sid(1, 4, 0x2A)?));
    record.push_conflict_range(b"\x40p-start".to_vec(), b"\x40p-end".to_vec());
    Ok(record)
}

/// The four records of the golden Round.
pub fn records() -> TestResult<[WriteSetRecord; 4]> {
    Ok([record0()?, record1()?, record2(), record3()?])
}

#[test]
fn test_the_golden_records_encode_to_the_golden_bytes() -> TestResult {
    let golden = golden_records()?;
    let built = records()?;
    assert_eq!(golden.len(), 4);
    // generate_bodies.py printed the records' lengths: 4,455, 192, 184 and 208 bytes.
    assert_eq!(
        golden.iter().map(Vec::len).collect::<Vec<_>>(),
        vec![4_455, 192, 184, 208]
    );
    for (record, bytes) in built.iter().zip(&golden) {
        assert_eq!(&record.encode(), bytes);
    }
    Ok(())
}

#[test]
fn test_a_transactions_record_keeps_its_fields_and_sections() -> TestResult {
    let record = record0()?;
    assert_eq!(record.kind(), RecordKind::Transaction);
    assert_eq!(
        (
            record.namespace(),
            record.catalog(),
            record.read_version(),
            record.request(),
            record.principal()
        ),
        (3, 11, (7 << 22) | 50, 7, 9)
    );
    assert_eq!(record.session(), SESSION);
    assert_eq!(
        record.inserted(),
        &[
            (
                b"quad-a".to_vec(),
                Some(sid(2, 5, 0x2A)?.to_be_bytes().to_vec())
            ),
            (b"quad-b".to_vec(), Some(Vec::new())),
        ]
    );
    assert_eq!(
        record.removed(),
        &[(
            b"quad-c".to_vec(),
            Some(sid(2, 6, 0x2A)?.to_be_bytes().to_vec())
        )]
    );
    assert_eq!(
        record.statement_records(),
        &[
            (b"g+stmt-5".to_vec(), Some(b"record-5".to_vec())),
            (b"g+stmt-6".to_vec(), None),
        ]
    );
    assert!(record.operations().is_empty());
    assert_eq!(
        record.registrations(),
        &[registration(sid(1, 9, 0x40)?, 1, 3)?]
    );
    assert_eq!(record.commit_record().len(), 4_000);
    assert_eq!(record.conflict_points(), &[b"\x10key-a".to_vec()]);
    assert_eq!(
        record.conflict_statements(),
        &[(b"\x10list-a".to_vec(), b"\x01obj".to_vec())]
    );
    assert_eq!(
        record.conflict_ranges(),
        &[(b"\x30a".to_vec(), b"\x30b".to_vec())]
    );
    assert_eq!(
        record.results(),
        &[
            OperationResult::for_node(0, true),
            OperationResult::counter(-5),
            OperationResult::compare_and_set(),
        ]
    );
    assert_eq!(
        record.result_terms().terms(),
        &[RequestTerm::Iri("http://ex.org/x".to_string())]
    );
    Ok(())
}

/// An entity a home got or created for a registration naming no bucket group is a kind-2 record with no registration,
/// every other field and section empty, which its view reads back.
#[test]
fn test_an_entity_record_is_a_registration_kind_with_no_registration() -> TestResult {
    let entity = WriteSetRecord::entity(5);
    assert_eq!(entity.kind(), RecordKind::Registration);
    assert_eq!(
        (entity.namespace(), entity.catalog(), entity.read_version()),
        (5, 0, 0)
    );
    assert!(entity.registrations().is_empty());
    let bytes = entity.encode();
    let view = WriteSetRecordView::new(&bytes)?;
    assert_eq!(
        (view.kind(), view.namespace()),
        (RecordKind::Registration, 5)
    );
    assert_eq!(view.registration(0), None);
    Ok(())
}

#[test]
fn test_the_other_kinds_leave_their_fields_zero() -> TestResult {
    let home = record1()?;
    assert_eq!(home.kind(), RecordKind::Registration);
    assert_eq!(
        (
            home.namespace(),
            home.catalog(),
            home.read_version(),
            home.request(),
            home.principal()
        ),
        (3, 0, 0, 0, 0)
    );
    assert_eq!(home.session(), [0; 16]);
    assert_eq!(
        home.registrations(),
        &[registration(sid(1, 12, 0x80)?, 2, 1)?]
    );
    assert!(home.operations().is_empty());
    let build = record2();
    assert_eq!(build.kind(), RecordKind::Build);
    assert_eq!((build.namespace(), build.catalog()), (4, 12));
    assert_eq!(
        build.operations(),
        &[RecordOperation::CatalogTakesEffect(12)]
    );
    assert!(build.registrations().is_empty());
    let drop = record3()?;
    assert_eq!(drop.kind(), RecordKind::Transaction);
    assert_eq!(drop.read_version(), 0);
    assert_eq!(
        drop.operations(),
        &[RecordOperation::DropPredicate(sid(1, 4, 0x2A)?)]
    );
    Ok(())
}

#[test]
fn test_an_empty_transaction_record_is_its_fixed_part_and_table() -> TestResult {
    let bytes = WriteSetRecord::transaction(0).encode();
    // 64 bytes of fixed part, then 12 sections' offsets and lengths, every section empty at offset 160.
    let mut expected = vec![1u8];
    expected.resize(64, 0);
    for _ in 0..12 {
        expected.extend_from_slice(&160u32.to_le_bytes());
        expected.extend_from_slice(&0u32.to_le_bytes());
    }
    assert_eq!(bytes, expected);
    let view = WriteSetRecordView::new(&bytes)?;
    assert_eq!(view.to_record(), WriteSetRecord::transaction(0));
    Ok(())
}

#[test]
fn test_node_results_intern_their_nodes_in_result_order() -> TestResult {
    let mut record = WriteSetRecord::transaction(1);
    record.push_node_result("http://ex.org/b", false);
    record.push_result(OperationResult::counter(3));
    record.push_node_result("http://ex.org/a", true);
    record.push_node_result("http://ex.org/b", true);
    assert_eq!(
        record.results(),
        &[
            OperationResult::for_node(0, false),
            OperationResult::counter(3),
            OperationResult::for_node(1, true),
            OperationResult::for_node(0, true),
        ]
    );
    assert_eq!(
        record.result_terms().terms(),
        &[
            RequestTerm::Iri("http://ex.org/b".to_string()),
            RequestTerm::Iri("http://ex.org/a".to_string()),
        ]
    );
    let bytes = record.encode();
    let view = WriteSetRecordView::new(&bytes)?;
    assert_eq!(view.result_terms().len(), 2);
    assert_eq!(view.to_record(), record);
    Ok(())
}

#[test]
fn test_a_later_setting_replaces_the_earlier() -> TestResult {
    let mut record = WriteSetRecord::transaction(5);
    record.set_catalog(1);
    record.set_catalog(2);
    record.set_read_version(10);
    record.set_read_version(12);
    record.set_session([7; 16], 1);
    record.set_session([8; 16], 2);
    record.set_principal(4);
    record.set_principal(6);
    record.set_commit_record(b"a".to_vec());
    record.set_commit_record(b"bc".to_vec());
    assert_eq!(
        (
            record.catalog(),
            record.read_version(),
            record.request(),
            record.principal()
        ),
        (2, 12, 2, 6)
    );
    assert_eq!(record.session(), [8; 16]);
    assert_eq!(record.commit_record(), b"bc");
    Ok(())
}

#[test]
fn test_a_builds_catalog_and_its_operations_version_are_apart() -> TestResult {
    // The golden build names catalog 12 in both places; here the in-effect catalog, 13, differs from the version the
    // build completes, 12, so neither can stand in for the other.
    let mut record = WriteSetRecord::build(
        4,
        RecordOperation::CompleteBuild {
            version: 12,
            outcome: ultragraphdb_protocol_wire::BuildOutcome::Undo,
        },
    );
    record.set_catalog(13);
    let bytes = record.encode();
    assert_eq!(bytes[16..24], 13u64.to_le_bytes());
    let (operations, len) = crate::utils::golden_bodies::section(&bytes, 64, 3);
    assert_eq!(len, 24);
    assert_eq!(bytes[operations..operations + 2], 7u16.to_le_bytes());
    assert_eq!(bytes[operations + 8..operations + 16], 12u64.to_le_bytes());
    let view = WriteSetRecordView::new(&bytes)?;
    assert_eq!(view.catalog(), 13);
    assert_eq!(
        view.operation(0),
        Some(RecordOperation::CompleteBuild {
            version: 12,
            outcome: ultragraphdb_protocol_wire::BuildOutcome::Undo
        })
    );
    Ok(())
}
