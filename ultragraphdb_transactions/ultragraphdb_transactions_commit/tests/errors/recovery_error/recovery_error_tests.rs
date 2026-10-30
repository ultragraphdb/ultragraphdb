/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_error_log::LogError;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_protocol_wire::WriteSetRecordView;
use ultragraphdb_store_apply::ChangesetError;
use ultragraphdb_transactions_commit::{RecoveryError, RecoveryErrorEnum};

/// The reasons that carry a cause build their variant around it, display their reason and the position they name before
/// the cause's own text, and give the cause as their source.
#[test]
fn test_each_reason_with_a_cause_carries_and_displays_it() {
    let log = LogError::Poisoned();
    let round = WriteSetRecordView::new(&[1, 2, 3]).expect_err("three bytes are no body");
    let apply = ChangesetError::Rebuilding("p");
    let sessions = StoreError::Corrupted(b"k".to_vec(), "bad".to_string());
    let cases = [
        (
            RecoveryError::Log(log.clone()),
            RecoveryErrorEnum::Log { cause: log.clone() },
            format!("the log could not be read: {log}"),
            log.to_string(),
        ),
        (
            RecoveryError::Round(7, round.clone()),
            RecoveryErrorEnum::Round {
                position: 7,
                cause: round.clone(),
            },
            format!("the round at position 7 is malformed: {round}"),
            round.to_string(),
        ),
        (
            RecoveryError::Apply(9, apply.clone()),
            RecoveryErrorEnum::Apply {
                position: 9,
                cause: apply.clone(),
            },
            format!("the store refused the round at position 9: {apply}"),
            apply.to_string(),
        ),
        (
            RecoveryError::Sessions(sessions.clone()),
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
}

/// The reasons about versions name them, and have no source.
#[test]
fn test_each_reason_about_versions_names_them_without_a_source() {
    let not_in_log = RecoveryError::StoreNotInLog(30);
    assert_eq!(
        not_in_log.0,
        RecoveryErrorEnum::StoreNotInLog { version: 30 }
    );
    assert_eq!(
        not_in_log.to_string(),
        "the store's applied version 30 is no round's last version in the log: the store is ahead of the log or was \
         written outside it"
    );
    let gap = RecoveryError::Gap(5);
    assert_eq!(gap.0, RecoveryErrorEnum::Gap { checkpoint: 5 });
    assert_eq!(
        gap.to_string(),
        "the store is empty and the log was trimmed through position 5: the rounds it covered are lost"
    );
    // The spec's "refuses to start, naming 30 and 40".
    let behind = RecoveryError::BehindCheckpoint(30, 40);
    assert_eq!(
        behind.0,
        RecoveryErrorEnum::BehindCheckpoint {
            applied: 30,
            checkpoint: 40
        }
    );
    assert_eq!(
        behind.to_string(),
        "the store's applied version 30 is below version 40, which the log's checkpoint covers: the rounds between them \
         are no longer in the log"
    );
    for error in [not_in_log, gap, behind] {
        assert!(error.source().is_none(), "{error:?}");
    }
}
