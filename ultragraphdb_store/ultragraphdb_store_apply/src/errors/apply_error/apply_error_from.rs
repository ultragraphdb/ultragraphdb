/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_catalog::SchemaError;
use ultragraphdb_core_keys::KeyError;
use ultragraphdb_core_tok::TokError;
use ultragraphdb_core_types::TypesError;
use ultragraphdb_error_num::NumError;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_hnsw::HnswError;
use ultragraphdb_store_posting::PostingError;

use super::ApplyError;

impl From<PostingError> for ApplyError {
    fn from(error: PostingError) -> Self {
        Self::Posting(error)
    }
}

impl From<StoreError> for ApplyError {
    fn from(error: StoreError) -> Self {
        Self::Posting(PostingError::Store(error))
    }
}

impl From<SchemaError> for ApplyError {
    fn from(error: SchemaError) -> Self {
        Self::Schema(error)
    }
}

impl From<KeyError> for ApplyError {
    fn from(error: KeyError) -> Self {
        Self::Key(error)
    }
}

impl From<TypesError> for ApplyError {
    fn from(error: TypesError) -> Self {
        Self::Types(error)
    }
}

impl From<TokError> for ApplyError {
    fn from(error: TokError) -> Self {
        Self::Tok(error)
    }
}

impl From<HnswError> for ApplyError {
    fn from(error: HnswError) -> Self {
        Self::Hnsw(error)
    }
}

impl From<NumError> for ApplyError {
    fn from(error: NumError) -> Self {
        Self::Num(error)
    }
}
