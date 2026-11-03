/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::Graph;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_apply::{ChangesetError, ChangesetErrorEnum, LogicalQuad};
use ultragraphdb_tool_bdd::{Context, TestResult};

/// The system ID of type 1, sequence `sequence` and bucket 5: `(1 << 112) | (sequence << 48) | (5 << 24)`.
fn id(sequence: u64) -> TestResult<SystemId> {
    SystemId::from_u128((1 << 112) | (u128::from(sequence) << 48) | (5 << 24)).context("the ID")
}

/// The 16 bytes of `id(sequence)`, from the layout: type 1 (`00 01`), the sequence's 8 bytes, bucket 5, 3 zero bytes.
fn id_bytes(sequence: u64) -> Vec<u8> {
    let mut bytes = vec![0x00, 0x01];
    bytes.extend_from_slice(&sequence.to_be_bytes());
    bytes.extend_from_slice(&[0, 0, 5, 0, 0, 0]);
    bytes
}

#[test]
fn test_a_statements_key_is_its_graph_subject_predicate_and_object() -> TestResult {
    let object = EncodedTerm::id(id(3)?);
    let quad = LogicalQuad::new(Graph::Default, id(1)?, id(2)?, object.clone(), None);
    let mut key = vec![0; 16];
    key.extend(id_bytes(1));
    key.extend(id_bytes(2));
    key.extend_from_slice(object.as_bytes());
    assert_eq!(quad.key(), key);
    assert!(quad.value().is_empty());
    assert_eq!(LogicalQuad::DERIVATION, 1);
    let named = LogicalQuad::new(Graph::Named(id(9)?), id(1)?, id(2)?, object, Some(id(4)?));
    assert_eq!(named.key().get(..16), Some(id_bytes(9).as_slice()));
    assert_eq!(named.value(), id_bytes(4));
    Ok(())
}

#[test]
fn test_a_statement_reads_back_from_its_key_and_value() -> TestResult {
    for statement in [None, Some(id(4)?)] {
        let quad = LogicalQuad::new(
            Graph::Named(id(9)?),
            id(1)?,
            id(2)?,
            EncodedTerm::id(id(3)?),
            statement,
        );
        let read = LogicalQuad::decode(&quad.key(), &quad.value()).context("it decodes")?;
        assert_eq!(read, quad);
        assert_eq!(read.graph(), Graph::Named(id(9)?));
        assert_eq!((read.subject(), read.predicate()), (id(1)?, id(2)?));
        assert_eq!(read.object(), &EncodedTerm::id(id(3)?));
        assert_eq!(read.statement(), statement);
    }
    let default = LogicalQuad::new(
        Graph::Default,
        id(1)?,
        id(2)?,
        EncodedTerm::id(id(3)?),
        None,
    );
    assert_eq!(
        LogicalQuad::decode(&default.key(), &[])?.graph(),
        Graph::Default
    );
    Ok(())
}

/// Whether `refused` is the store's corruption error naming `key`.
fn corrupted_naming(refused: Result<LogicalQuad, ChangesetError>, key: &[u8]) -> bool {
    matches!(
        refused,
        Err(ChangesetError(ChangesetErrorEnum::Store(StoreError(StoreErrorEnum::Corrupted { key: k, .. }))))
            if k == key
    )
}

#[test]
fn test_a_malformed_statement_is_refused_as_corruption_naming_its_key() -> TestResult {
    let good = LogicalQuad::new(
        Graph::Default,
        id(1)?,
        id(2)?,
        EncodedTerm::id(id(3)?),
        None,
    )
    .key();
    let short = good.get(..47).context("47 bytes")?.to_vec();
    assert!(corrupted_naming(LogicalQuad::decode(&short, &[]), &short));
    let mut zero_subject = good.clone();
    if let Some(subject) = zero_subject.get_mut(16..32) {
        subject.fill(0);
    }
    assert!(corrupted_naming(
        LogicalQuad::decode(&zero_subject, &[]),
        &zero_subject
    ));
    let mut bad_graph = good.clone();
    if let Some(graph) = bad_graph.get_mut(..16) {
        graph.fill(0xFF);
    }
    assert!(corrupted_naming(
        LogicalQuad::decode(&bad_graph, &[]),
        &bad_graph
    ));
    let no_object = good.get(..48).context("48 bytes")?.to_vec();
    assert!(corrupted_naming(
        LogicalQuad::decode(&no_object, &[]),
        &no_object
    ));
    assert!(corrupted_naming(
        LogicalQuad::decode(&good, &[1, 2, 3]),
        &good
    ));
    assert!(corrupted_naming(
        LogicalQuad::decode(&good, &[0; 16]),
        &good
    ));
    Ok(())
}
