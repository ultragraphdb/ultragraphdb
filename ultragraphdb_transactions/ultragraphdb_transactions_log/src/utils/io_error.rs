/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;

use ultragraphdb_error_log::LogError;

/// The storage's error as [`LogError::Io`], naming the `operation` that failed.
pub(crate) fn io_error(operation: &'static str) -> impl Fn(io::Error) -> LogError {
    move |error| LogError::Io(operation, error.kind(), error.to_string())
}
