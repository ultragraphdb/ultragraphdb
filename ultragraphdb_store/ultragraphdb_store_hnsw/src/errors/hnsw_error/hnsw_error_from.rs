/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_options::OptionsError;
use ultragraphdb_store_posting::PostingError;

use super::HnswError;

impl From<PostingError> for HnswError {
    fn from(error: PostingError) -> Self {
        Self::Posting(error)
    }
}

impl From<OptionsError> for HnswError {
    fn from(error: OptionsError) -> Self {
        Self::Options(error)
    }
}
