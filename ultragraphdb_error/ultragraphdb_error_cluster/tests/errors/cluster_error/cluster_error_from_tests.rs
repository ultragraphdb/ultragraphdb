/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_error_invariant::InvariantError;
use ultragraphdb_error_log::LogError;

#[test]
fn test_a_log_error_converts_into_the_log_variant() {
    let error = ClusterError::from(LogError::poisoned());
    assert_eq!(error.0, ClusterErrorEnum::Log(LogError::poisoned()));
}

#[test]
fn test_an_invariant_error_converts_into_the_invariant_variant() {
    let error = ClusterError::from(InvariantError::violated("closes rise by one"));
    assert_eq!(
        error.0,
        ClusterErrorEnum::Invariant(InvariantError::violated("closes rise by one"))
    );
}

#[test]
fn test_question_mark_converts_a_log_error() {
    fn refuse() -> Result<(), ClusterError> {
        Err(LogError::sealed(3))?
    }
    assert_eq!(
        refuse(),
        Err(ClusterError(ClusterErrorEnum::Log(LogError::sealed(3))))
    );
}
