/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;
use std::panic::Location;

use crate::types::test_error::{TestError, TestResult};

/// Turns an absent value or a failed result into a [`TestError`] naming what was expected and the line that expected
/// it, where a test would otherwise `expect` it: `load(path).context("the golden file")?`. The test returns the error
/// rather than panicking, and the harness fails it on the `Err`.
pub trait Context<T> {
    /// The value, or a [`TestError`] saying `what` was expected and, for a failed result, its failure.
    ///
    /// # Errors
    ///
    /// The [`TestError`] `<what>: none (at <file>:<line>:<column>)` for `None`, and `<what>: <the error's text> (at
    /// <file>:<line>:<column>)` for an `Err`, naming the caller's line.
    fn context(self, what: &str) -> TestResult<T>;
}

impl<T> Context<T> for Option<T> {
    #[track_caller]
    fn context(self, what: &str) -> TestResult<T> {
        let at = Location::caller();
        self.ok_or_else(|| TestError::new(format!("{what}: none (at {at})")))
    }
}

impl<T, E: fmt::Display> Context<T> for Result<T, E> {
    #[track_caller]
    fn context(self, what: &str) -> TestResult<T> {
        let at = Location::caller();
        self.map_err(|e| TestError::new(format!("{what}: {e} (at {at})")))
    }
}
