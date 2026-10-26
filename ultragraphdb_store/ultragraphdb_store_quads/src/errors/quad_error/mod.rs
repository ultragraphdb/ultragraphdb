/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod quad_error_display;
mod quad_error_error;
mod quad_error_from;

use ultragraphdb_core_catalog::CatalogError;
use ultragraphdb_core_keys::QuadKeyError;
use ultragraphdb_core_mapping::MappingError;
use ultragraphdb_core_terms::TermError;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_posting::PostingError;

/// Why an operation of the quad store failed. The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq)]
pub enum QuadErrorEnum {
    Store(StoreError),
    Posting(PostingError),
    Term(TermError),
    Key(QuadKeyError),
    Mapping(MappingError),
    /// The catalog could not be read.
    Catalog(CatalogError),
    /// A stored UID or literal hash names no dictionary entry, or an entry cannot be read: `key` holds it.
    Corrupted {
        key: Vec<u8>,
        reason: String,
    },
    /// A commit since the write's read assigned an IRI the write creates: the write retries and finds its UID.
    Aborted,
    /// The UIDs below the entity-type bits are spent.
    UidsExhausted {
        namespace: u64,
    },
}

/// See [`QuadErrorEnum`].
#[derive(Debug, Clone, PartialEq)]
pub struct QuadError(pub QuadErrorEnum);

impl QuadError {
    #[allow(non_snake_case)]
    pub fn Corrupted(key: &[u8], reason: impl Into<String>) -> Self {
        Self(QuadErrorEnum::Corrupted {
            key: key.to_vec(),
            reason: reason.into(),
        })
    }

    #[allow(non_snake_case)]
    pub fn Aborted() -> Self {
        Self(QuadErrorEnum::Aborted)
    }

    #[allow(non_snake_case)]
    pub fn UidsExhausted(namespace: u64) -> Self {
        Self(QuadErrorEnum::UidsExhausted { namespace })
    }
}
