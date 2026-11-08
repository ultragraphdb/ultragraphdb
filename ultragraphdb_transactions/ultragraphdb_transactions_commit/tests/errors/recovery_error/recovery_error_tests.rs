/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::utils::fixture::Refused;
use std::error::Error;
use ultragraphdb_tool_bdd::TestResult;

use ultragraphdb_error_invariant::InvariantError;
use ultragraphdb_error_log::LogError;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_protocol_wire::WriteSetRecordView;
use ultragraphdb_store_apply::ChangesetError;
use ultragraphdb_transactions_commit::{RecoveryError, RecoveryErrorEnum};

/// The reasons that carry a cause build their variant around it, display their reason and the position they name before
/// the cause's own text, and give the cause as their source.
#[test]
fn test_each_reason_with_a_cause_carries_and_displays_it() -> TestResult {
    let log = LogError::poisoned();
    let round = WriteSetRecordView::new(&[1, 2, 3]).refused("three bytes are no body")?;
    let apply = ChangesetError::rebuilding("p");
    let sessions = StoreError::corrupted(b"k".to_vec(), "bad".to_string());
    let invariant = InvariantError::violated("a stored reply's term table holds IRIs alone");
    let cases = [
        (
            RecoveryError::log(log.clone()),
            RecoveryErrorEnum::Log { cause: log.clone() },
            format!("the log could not be read: {log}"),
            log.to_string(),
        ),
        (
            RecoveryError::round(7, round.clone()),
            RecoveryErrorEnum::Round {
                position: 7,
                cause: round.clone(),
            },
            format!("the round at position 7 is malformed: {round}"),
            round.to_string(),
        ),
        (
            RecoveryError::apply(9, apply.clone()),
            RecoveryErrorEnum::Apply {
                position: 9,
                cause: apply.clone(),
            },
            format!("the store refused the round at position 9: {apply}"),
            apply.to_string(),
        ),
        (
            RecoveryError::invariant(4, invariant),
            RecoveryErrorEnum::Invariant {
                position: 4,
                cause: invariant,
            },
            format!("replaying the round at position 4: {invariant}"),
            invariant.to_string(),
        ),
        (
            RecoveryError::sessions(sessions.clone()),
            RecoveryErrorEnum::Sessions {
                cause: sessions.clone(),
            },
            format!("the store's session table could not be read: {sessions}"),
            sessions.to_string(),
        ),
    ];
    for (error, variant, text, source) in cases {
        assert_eq!(error.0, variant);
        assert_eq!(error.to_string(), text);
        assert_eq!(error.source().map(|s| s.to_string()), Some(source));
    }
    Ok(())
}

/// The reasons about log positions name them, and have no source.
#[test]
fn test_each_reason_about_positions_names_them_without_a_source() -> TestResult {
    let not_in_log = RecoveryError::store_not_in_log(30);
    assert_eq!(
        not_in_log.0,
        RecoveryErrorEnum::StoreNotInLog { position: 30 }
    );
    assert_eq!(
        not_in_log.to_string(),
        "the store's applied log position 30 lies beyond the log's tail: the store is ahead of the log or was written \
         outside it"
    );
    let gap = RecoveryError::gap(5);
    assert_eq!(gap.0, RecoveryErrorEnum::Gap { position: 5 });
    assert_eq!(
        gap.to_string(),
        "the log has no round at position 5, between the store's applied position and the log's tail"
    );
    // The spec's "refuses to start, naming 30 and 40".
    let behind = RecoveryError::behind_checkpoint(30, 40);
    assert_eq!(
        behind.0,
        RecoveryErrorEnum::BehindCheckpoint {
            applied: 30,
            checkpoint: 40
        }
    );
    assert_eq!(
        behind.to_string(),
        "the store's applied log position 30 is below the log's checkpoint at position 40: the rounds between them are \
         no longer in the log"
    );
    let derivation = RecoveryError::derivation(7, 3);
    assert_eq!(
        derivation.0,
        RecoveryErrorEnum::Derivation {
            position: 7,
            derivation: 3
        }
    );
    assert_eq!(
        derivation.to_string(),
        "the round at position 7 names derivation 3, which this store does not know: nothing of it is applied"
    );
    let catalog = RecoveryError::catalog(9, 2, 5, "the metadata group holds no version 5");
    assert_eq!(
        catalog.0,
        RecoveryErrorEnum::Catalog {
            position: 9,
            tenant: 2,
            version: 5,
            reason: "the metadata group holds no version 5".to_string()
        }
    );
    assert_eq!(
        catalog.to_string(),
        "the round at position 9 starts catalog version 5 of tenant 2, which could not be read: the metadata group \
         holds no version 5"
    );
    for error in [not_in_log, gap, behind, derivation, catalog] {
        assert!(error.source().is_none(), "{error:?}");
    }
    Ok(())
}

/// A failed read of the applied position carries the store's error.
#[test]
fn test_an_unreadable_applied_position_carries_the_stores_error() -> TestResult {
    let cause = ultragraphdb_error_store::StoreError::corrupted(b"k".to_vec(), "bad".to_string());
    let error = RecoveryError::applied_position(cause.clone());
    assert_eq!(
        error.0,
        RecoveryErrorEnum::AppliedPosition {
            cause: cause.clone()
        }
    );
    assert_eq!(
        error.to_string(),
        format!("the store's applied log position could not be read: {cause}")
    );
    assert_eq!(
        error.source().map(|s| s.to_string()),
        Some(cause.to_string())
    );
    Ok(())
}
