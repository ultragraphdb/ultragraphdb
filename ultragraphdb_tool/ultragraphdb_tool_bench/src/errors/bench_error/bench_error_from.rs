/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_apply::ChangesetError;
use ultragraphdb_store_posting::PostingError;

use crate::errors::bench_error::BenchError;

impl From<ChangesetError> for BenchError {
    fn from(error: ChangesetError) -> Self {
        Self::Apply(error)
    }
}

impl From<StoreError> for BenchError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<PostingError> for BenchError {
    fn from(error: PostingError) -> Self {
        Self::Apply(error.into())
    }
}

impl From<std::io::Error> for BenchError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}
