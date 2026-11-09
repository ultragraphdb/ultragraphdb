/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod bench_error_display;
mod bench_error_error;
mod bench_error_from;

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_apply::ChangesetError;

/// Why a run of `store_bench`, `wal_bench`, `client_bench` or `alloc_bench` stopped.
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
    /// The client's connection or one of its requests failed, as its text.
    Client(String),
    /// A declaration's index build had not ended after `waited_ms` milliseconds.
    BuildStuck {
        /// How long the run waited.
        waited_ms: u64,
    },
}

impl BenchError {
    /// The arguments name no run it can make, for `reason`.
    #[must_use]
    pub fn usage(reason: String) -> Self {
        Self(BenchErrorEnum::Usage(reason))
    }

    /// The applier refused an operation with `error`.
    #[must_use]
    pub fn apply(error: ChangesetError) -> Self {
        Self(BenchErrorEnum::Apply(error))
    }

    /// The store refused an operation with `error`.
    #[must_use]
    pub fn store(error: StoreError) -> Self {
        Self(BenchErrorEnum::Store(error))
    }

    /// A file of the run could not be read or written, for `reason`.
    #[must_use]
    pub fn io(reason: String) -> Self {
        Self(BenchErrorEnum::Io(reason))
    }

    /// The commit pipeline, a transaction or the write-ahead log refused an operation, as its text `reason`.
    #[must_use]
    pub fn commit(reason: String) -> Self {
        Self(BenchErrorEnum::Commit(reason))
    }

    /// The client's connection or one of its requests failed, as its text `reason`.
    #[must_use]
    pub fn client(reason: String) -> Self {
        Self(BenchErrorEnum::Client(reason))
    }

    /// A declaration's index build had not ended after `waited_ms` milliseconds.
    #[must_use]
    pub fn build_stuck(waited_ms: u64) -> Self {
        Self(BenchErrorEnum::BuildStuck { waited_ms })
    }
}
