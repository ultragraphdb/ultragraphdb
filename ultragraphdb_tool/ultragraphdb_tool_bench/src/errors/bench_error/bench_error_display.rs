/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::bench_error::{BenchError, BenchErrorEnum};

impl fmt::Display for BenchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            BenchErrorEnum::Usage(reason) => write!(f, "usage: {reason}"),
            BenchErrorEnum::Apply(error) => write!(f, "apply: {error}"),
            BenchErrorEnum::Store(error) => write!(f, "store: {error}"),
            BenchErrorEnum::Io(reason) => write!(f, "i/o: {reason}"),
        }
    }
}
