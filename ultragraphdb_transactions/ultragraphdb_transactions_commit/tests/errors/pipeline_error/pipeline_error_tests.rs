/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;
use ultragraphdb_tool_bdd::TestResult;

use ultragraphdb_error_log::LogError;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_apply::ChangesetError;
use ultragraphdb_transactions_commit::{PipelineError, PipelineErrorEnum};
use ultragraphdb_transactions_txn::TxnError;

/// Each constructor builds its variant around its cause, displays its reason before the cause's own text, and gives
/// the cause as its source.
#[test]
fn test_each_reason_carries_and_displays_its_cause() -> TestResult {
    let log = LogError::poisoned();
    let apply = ChangesetError::rebuilding("p");
    let sessions = StoreError::corrupted(b"k".to_vec(), "bad".to_string());
    let build = TxnError::ended();
    let cases = [
        (
            PipelineError::log(log.clone()),
            PipelineErrorEnum::Log { cause: log.clone() },
            format!("the log refused or failed a round: {log}"),
            log.to_string(),
        ),
        (
            PipelineError::apply(apply.clone()),
            PipelineErrorEnum::Apply {
                cause: apply.clone(),
            },
            format!("the store refused a synced round: {apply}"),
            apply.to_string(),
        ),
        (
            PipelineError::sessions(sessions.clone()),
            PipelineErrorEnum::Sessions {
                cause: sessions.clone(),
            },
            format!("the store's session table could not be read: {sessions}"),
            sessions.to_string(),
        ),
        (
            PipelineError::build_end(build.clone()),
            PipelineErrorEnum::BuildEnd {
                cause: build.clone(),
            },
            format!("a build's end could not be proposed: {build}"),
            build.to_string(),
        ),
    ];
    for (error, variant, text, source) in cases {
        assert_eq!(error.0, variant);
        assert_eq!(error.to_string(), text);
        assert_eq!(error.source().map(|s| s.to_string()), Some(source));
    }
    Ok(())
}
