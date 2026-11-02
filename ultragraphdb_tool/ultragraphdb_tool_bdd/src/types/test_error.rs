/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

/// Why a test failed: what went wrong, and where. A test or a scenario step returns [`TestResult`] and propagates a
/// failure with `?`, so it fails naming its cause without `unwrap`, `expect` or `panic!`. Any
/// `std::error::Error` converts into it; an absent value or another failure does through [`Context`](crate::Context).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TestError(String);

/// What a test or a scenario step returns.
pub type TestResult<T = ()> = Result<T, TestError>;

impl TestError {
    /// A failure for `reason`.
    pub fn new(reason: impl Into<String>) -> Self {
        Self(reason.into())
    }

    /// What went wrong.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.0
    }
}

impl<E: std::error::Error> From<E> for TestError {
    fn from(error: E) -> Self {
        Self(error.to_string())
    }
}

impl fmt::Display for TestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
