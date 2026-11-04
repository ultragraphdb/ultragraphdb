/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_cluster_partition::{PartitionError, PartitionErrorEnum};
use ultragraphdb_protocol_wire::ConflictKind;
use ultragraphdb_store_apply::ChangesetError;
use ultragraphdb_store_quads::QuadError;
use ultragraphdb_transactions_commit::{CommitError, PipelineError};
use ultragraphdb_transactions_txn::TxnError;

#[test]
fn test_each_constructor_builds_its_variant_and_names_its_fields() {
    let cases = [
        (
            PartitionError::wrong_partition(30_000, 2),
            PartitionErrorEnum::WrongPartition {
                bucket: 30_000,
                owner: 2,
            },
            ["30000", "2"],
        ),
        (
            PartitionError::cross_partition(1, 2),
            PartitionErrorEnum::CrossPartition {
                partition: 1,
                other: 2,
            },
            ["1", "2"],
        ),
        (
            PartitionError::timed_out(3),
            PartitionErrorEnum::TimedOut { partition: 3 },
            ["3", "3"],
        ),
        (
            PartitionError::refused(26),
            PartitionErrorEnum::Refused { outcome: 26 },
            ["26", "26"],
        ),
    ];
    for (error, variant, named) in cases {
        assert_eq!(error.0, variant);
        let text = error.to_string();
        assert!(named.iter().all(|n| text.contains(n)), "{text}");
        assert!(error.source().is_none(), "{text} wraps nothing");
    }
}

#[test]
fn test_a_wrapped_error_is_its_source_and_its_text() {
    let txn = TxnError::cross_partition(1, 2);
    let wrapped = PartitionError::from(txn.clone());
    assert_eq!(wrapped.0, PartitionErrorEnum::Txn(txn.clone()));
    assert_eq!(wrapped.to_string(), txn.to_string());
    assert_eq!(
        wrapped.source().map(ToString::to_string),
        Some(txn.to_string())
    );
    let quad = QuadError::not_owned("http://ex.org/b", 7);
    let from_store = PartitionError::from(quad.clone());
    assert_eq!(from_store.0, PartitionErrorEnum::Quad(quad.clone()));
    assert_eq!(from_store.to_string(), quad.to_string());
    assert!(from_store.source().is_some());
}

#[test]
fn test_the_applier_pipeline_and_commit_step_errors_are_wrapped() {
    let changeset = ChangesetError::no_index("http://ex.org/p", "term");
    let from_applier = PartitionError::from(changeset.clone());
    assert_eq!(
        from_applier.0,
        PartitionErrorEnum::Changeset(changeset.clone())
    );
    assert_eq!(from_applier.to_string(), changeset.to_string());
    assert!(from_applier.source().is_some());
    let pipeline = PipelineError::apply(changeset);
    let from_pipeline = PartitionError::from(pipeline.clone());
    assert_eq!(
        from_pipeline.0,
        PartitionErrorEnum::Pipeline(pipeline.clone())
    );
    assert_eq!(from_pipeline.to_string(), pipeline.to_string());
    assert!(from_pipeline.source().is_some());
    let refusal = CommitError::conflict(ConflictKind::List);
    let from_step = PartitionError::from(refusal.clone());
    assert_eq!(from_step.0, PartitionErrorEnum::Commit(refusal.clone()));
    assert_eq!(from_step.to_string(), refusal.to_string());
    assert!(from_step.source().is_some());
}
