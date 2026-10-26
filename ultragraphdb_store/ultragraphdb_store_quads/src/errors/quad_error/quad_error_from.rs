/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_catalog::CatalogError;
use ultragraphdb_core_keys::QuadKeyError;
use ultragraphdb_core_mapping::MappingError;
use ultragraphdb_core_terms::TermError;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_posting::PostingError;

use crate::errors::quad_error::{QuadError, QuadErrorEnum};

impl From<StoreError> for QuadError {
    fn from(e: StoreError) -> Self {
        Self(QuadErrorEnum::Store(e))
    }
}

impl From<PostingError> for QuadError {
    fn from(e: PostingError) -> Self {
        Self(QuadErrorEnum::Posting(e))
    }
}

impl From<TermError> for QuadError {
    fn from(e: TermError) -> Self {
        Self(QuadErrorEnum::Term(e))
    }
}

impl From<QuadKeyError> for QuadError {
    fn from(e: QuadKeyError) -> Self {
        Self(QuadErrorEnum::Key(e))
    }
}

impl From<CatalogError> for QuadError {
    fn from(e: CatalogError) -> Self {
        Self(QuadErrorEnum::Catalog(e))
    }
}

impl From<MappingError> for QuadError {
    fn from(e: MappingError) -> Self {
        Self(QuadErrorEnum::Mapping(e))
    }
}
