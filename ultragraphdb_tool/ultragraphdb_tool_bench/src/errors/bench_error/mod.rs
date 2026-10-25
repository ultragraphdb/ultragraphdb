/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod bench_error_display;
mod bench_error_error;
mod bench_error_from;

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_apply::ApplyError;

/// Why a run of `store_bench` stopped.
#[derive(Debug)]
pub struct BenchError(pub BenchErrorEnum);

#[derive(Debug)]
pub enum BenchErrorEnum {
    /// The arguments name no run it can make.
    Usage(String),
    /// The applier refused an operation.
    Apply(ApplyError),
    /// The store refused an operation.
    Store(StoreError),
    /// A file of the run could not be read or written.
    Io(String),
}

#[allow(non_snake_case)]
impl BenchError {
    pub fn Usage(reason: String) -> Self {
        Self(BenchErrorEnum::Usage(reason))
    }

    pub fn Apply(error: ApplyError) -> Self {
        Self(BenchErrorEnum::Apply(error))
    }

    pub fn Store(error: StoreError) -> Self {
        Self(BenchErrorEnum::Store(error))
    }

    pub fn Io(reason: String) -> Self {
        Self(BenchErrorEnum::Io(reason))
    }
}
