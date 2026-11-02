/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod bench_error_display;
mod bench_error_error;
mod bench_error_from;

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_apply::ChangesetError;

/// Why a run of `store_bench` or `wal_bench` stopped.
#[derive(Debug)]
pub struct BenchError(pub BenchErrorEnum);

/// The reasons a run stops, one per [`BenchError`] constructor.
#[derive(Debug)]
pub enum BenchErrorEnum {
    /// The arguments name no run it can make.
    Usage(String),
    /// The applier refused an operation.
    Apply(ChangesetError),
    /// The store refused an operation.
    Store(StoreError),
    /// A file of the run could not be read or written.
    Io(String),
    /// The commit pipeline, a transaction or the write-ahead log refused an operation, as its text.
    Commit(String),
}

impl BenchError {
    /// The arguments name no run it can make, for `reason`.
    pub fn usage(reason: String) -> Self {
        Self(BenchErrorEnum::Usage(reason))
    }

    /// The applier refused an operation with `error`.
    pub fn apply(error: ChangesetError) -> Self {
        Self(BenchErrorEnum::Apply(error))
    }

    /// The store refused an operation with `error`.
    pub fn store(error: StoreError) -> Self {
        Self(BenchErrorEnum::Store(error))
    }

    /// A file of the run could not be read or written, for `reason`.
    pub fn io(reason: String) -> Self {
        Self(BenchErrorEnum::Io(reason))
    }

    /// The commit pipeline, a transaction or the write-ahead log refused an operation, as its text `reason`.
    pub fn commit(reason: String) -> Self {
        Self(BenchErrorEnum::Commit(reason))
    }
}
