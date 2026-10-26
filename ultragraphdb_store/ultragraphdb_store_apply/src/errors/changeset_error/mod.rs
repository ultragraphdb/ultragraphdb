/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod changeset_error_display;
mod changeset_error_error;
mod changeset_error_from;

use ultragraphdb_core_catalog::CatalogError;
use ultragraphdb_core_mapping::MappingError;
use ultragraphdb_core_terms::TermError;
use ultragraphdb_core_tok::TokError;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_posting::PostingError;
use ultragraphdb_store_quads::QuadError;

/// Why a changeset was refused, or could not be applied. A refused changeset writes nothing. The classification is
/// the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq)]
pub enum ChangesetErrorEnum {
    /// The catalog refused a write or a declaration.
    Catalog(CatalogError),
    /// The canonical model refused a change: a statement named that is not held, or held by several.
    Mapping(MappingError),
    /// The quad store failed, or refused the write at commit: `Transaction has been aborted. Please retry`.
    Quad(QuadError),
    Posting(PostingError),
    Store(StoreError),
    Tok(TokError),
    /// A literal the write names cannot be encoded, a vector not in its canonical form above all; `predicate` names
    /// the statement's predicate.
    Literal {
        predicate: String,
        error: TermError,
    },
    /// A unique predicate's value another subject already holds in the graph.
    Unique {
        predicate: String,
        value: String,
        holder: String,
    },
}

/// See [`ChangesetErrorEnum`].
#[derive(Debug, Clone, PartialEq)]
pub struct ChangesetError(pub ChangesetErrorEnum);

impl ChangesetError {
    #[allow(non_snake_case)]
    pub fn Literal(predicate: &str, error: TermError) -> Self {
        Self(ChangesetErrorEnum::Literal {
            predicate: predicate.to_string(),
            error,
        })
    }

    #[allow(non_snake_case)]
    pub fn Unique(predicate: &str, value: String, holder: String) -> Self {
        Self(ChangesetErrorEnum::Unique {
            predicate: predicate.to_string(),
            value,
            holder,
        })
    }
}
