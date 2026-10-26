/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_catalog::CatalogError;
use ultragraphdb_core_mapping::MappingError;
use ultragraphdb_core_tok::TokError;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_hnsw::HnswError;
use ultragraphdb_store_posting::PostingError;
use ultragraphdb_store_quads::QuadError;

use crate::errors::changeset_error::{ChangesetError, ChangesetErrorEnum};

impl From<CatalogError> for ChangesetError {
    fn from(e: CatalogError) -> Self {
        Self(ChangesetErrorEnum::Catalog(e))
    }
}

impl From<MappingError> for ChangesetError {
    fn from(e: MappingError) -> Self {
        Self(ChangesetErrorEnum::Mapping(e))
    }
}

impl From<QuadError> for ChangesetError {
    fn from(e: QuadError) -> Self {
        Self(ChangesetErrorEnum::Quad(e))
    }
}

impl From<PostingError> for ChangesetError {
    fn from(e: PostingError) -> Self {
        Self(ChangesetErrorEnum::Posting(e))
    }
}

impl From<StoreError> for ChangesetError {
    fn from(e: StoreError) -> Self {
        Self(ChangesetErrorEnum::Store(e))
    }
}

impl From<TokError> for ChangesetError {
    fn from(e: TokError) -> Self {
        Self(ChangesetErrorEnum::Tok(e))
    }
}

impl From<HnswError> for ChangesetError {
    fn from(e: HnswError) -> Self {
        Self(ChangesetErrorEnum::Hnsw(e))
    }
}
