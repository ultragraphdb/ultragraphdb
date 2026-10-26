/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreError;

use super::PostingError;

impl From<StoreError> for PostingError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}
