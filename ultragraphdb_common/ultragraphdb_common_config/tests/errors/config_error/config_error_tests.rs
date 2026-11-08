/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_common_config::ConfigError;
use ultragraphdb_common_config_check::Defect;
use ultragraphdb_error_log::LogError;
use ultragraphdb_tool_bdd::{Context, TestResult};

/// A refusal lists its defects one to a line, as each reads alone, with no line before the first.
#[test]
fn test_a_refusal_lists_one_defect_to_a_line() {
    let first = Defect::new("it holds 0", "it must be at least 1");
    let second = Defect::new("it is absent", "a storage requires it");
    let both = ConfigError::refused(vec![first.clone(), second.clone()]);
    assert_eq!(both.to_string(), format!("{first}\n{second}"));
    assert_eq!(
        ConfigError::refused(vec![first.clone()]).to_string(),
        first.to_string()
    );
}

/// The log's error is the source of the error it becomes; no other variant has one.
#[test]
fn test_the_log_error_is_the_source() -> TestResult {
    let log = LogError::busy(3);
    let wrapped = ConfigError::from(log.clone());
    let source = wrapped.source().context("the log's error")?;
    assert_eq!(source.to_string(), log.to_string());
    assert!(ConfigError::stale("ci", 1, 2).source().is_none());
    Ok(())
}
