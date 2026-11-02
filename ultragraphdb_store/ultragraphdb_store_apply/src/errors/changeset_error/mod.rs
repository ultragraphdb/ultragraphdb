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
use ultragraphdb_error_invariant::InvariantError;
use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_hnsw::HnswError;
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
    /// A posting list could not be read or written.
    Posting(PostingError),
    /// The store failed.
    Store(StoreError),
    /// A tokenizer refused a value.
    Tok(TokError),
    /// The vector index failed.
    Hnsw(HnswError),
    /// An invariant of the applier does not hold: a bug in UltraGraphDB, never the changeset.
    Invariant(InvariantError),
    /// A literal the write names cannot be encoded, a vector not in its canonical form above all; `predicate` names
    /// the statement's predicate.
    Literal {
        /// The statement's predicate.
        predicate: String,
        /// Why the literal cannot be encoded.
        error: TermError,
    },
    /// A unique predicate's value another subject already holds in the graph.
    Unique {
        /// The unique predicate.
        predicate: String,
        /// The value written, as an N-Triples term.
        value: String,
        /// The subject already holding it.
        holder: String,
    },
    /// A declaration the predicate's data contradicts: `subject` holds what the declaration refuses, as `reason`
    /// states.
    Contradicted {
        /// The predicate declared.
        predicate: String,
        /// The subject holding what the declaration refuses.
        subject: String,
        /// What the subject holds that the declaration refuses.
        reason: String,
    },
    /// A declaration that needs an index built, or that changes the predicate being built, while a build runs.
    Rebuilding {
        /// The predicate declared.
        predicate: String,
    },
    /// A vector index whose options its factory refuses.
    VectorOption {
        /// The factory refusing the options.
        factory: String,
        /// The factory's reason.
        error: String,
    },
    /// A vector whose dimension is not the one the predicate's vector index declares.
    Dimension {
        /// The vector's predicate.
        predicate: String,
        /// The dimension the index declares.
        declared: usize,
        /// The dimension of the vector written.
        written: usize,
    },
    /// A vector index declared without its dimension.
    NoDimension {
        /// The predicate declared.
        predicate: String,
    },
    /// A vector search or index pass on a predicate the catalog declares with no vector index.
    NoVectorIndex {
        /// The predicate named.
        predicate: String,
    },
    /// A compare-and-set whose expected value is not held, or a counter add on a value that is no integer or would
    /// overflow: `found` is the value held, an N-Triples term, `None` when none is (7.5).
    OperationFailed {
        /// The value held, `None` when none is.
        found: Option<String>,
    },
    /// An insert-if-absent or upsert-and-link on a predicate the catalog does not declare unique (7.5).
    NotUnique {
        /// The predicate named.
        predicate: String,
    },
}

/// See [`ChangesetErrorEnum`].
#[derive(Debug, Clone, PartialEq)]
pub struct ChangesetError(pub ChangesetErrorEnum);

impl ChangesetError {
    /// A literal of `predicate` that cannot be encoded, for `error`.
    #[allow(non_snake_case)]
    pub fn Literal(predicate: &str, error: TermError) -> Self {
        Self(ChangesetErrorEnum::Literal {
            predicate: predicate.to_string(),
            error,
        })
    }

    /// A unique `predicate`'s `value` that `holder` already holds.
    #[allow(non_snake_case)]
    pub fn Unique(predicate: &str, value: String, holder: String) -> Self {
        Self(ChangesetErrorEnum::Unique {
            predicate: predicate.to_string(),
            value,
            holder,
        })
    }

    /// A declaration of `predicate` that `subject`'s data contradicts, as `reason` states.
    #[allow(non_snake_case)]
    pub fn Contradicted(predicate: &str, subject: String, reason: &str) -> Self {
        Self(ChangesetErrorEnum::Contradicted {
            predicate: predicate.to_string(),
            subject,
            reason: reason.to_string(),
        })
    }

    /// A declaration of `predicate` refused while a build runs.
    #[allow(non_snake_case)]
    pub fn Rebuilding(predicate: &str) -> Self {
        Self(ChangesetErrorEnum::Rebuilding {
            predicate: predicate.to_string(),
        })
    }

    /// Vector index options `factory` refuses, for `error`.
    #[allow(non_snake_case)]
    pub fn VectorOption(factory: &str, error: String) -> Self {
        Self(ChangesetErrorEnum::VectorOption {
            factory: factory.to_string(),
            error,
        })
    }

    /// A failed compare-and-set or counter add, `found` the value held.
    #[allow(non_snake_case)]
    pub fn OperationFailed(found: Option<String>) -> Self {
        Self(ChangesetErrorEnum::OperationFailed { found })
    }

    /// An insert-if-absent or upsert-and-link on `predicate`, which is not declared unique.
    #[allow(non_snake_case)]
    pub fn NotUnique(predicate: &str) -> Self {
        Self(ChangesetErrorEnum::NotUnique {
            predicate: predicate.to_string(),
        })
    }

    /// A vector of dimension `written` for `predicate`, whose index declares `declared`.
    #[allow(non_snake_case)]
    pub fn Dimension(predicate: &str, declared: usize, written: usize) -> Self {
        Self(ChangesetErrorEnum::Dimension {
            predicate: predicate.to_string(),
            declared,
            written,
        })
    }

    /// A vector index on `predicate` declared without its dimension.
    #[allow(non_snake_case)]
    pub fn NoDimension(predicate: &str) -> Self {
        Self(ChangesetErrorEnum::NoDimension {
            predicate: predicate.to_string(),
        })
    }

    /// A vector search or index pass on `predicate`, which declares no vector index.
    #[allow(non_snake_case)]
    pub fn NoVectorIndex(predicate: &str) -> Self {
        Self(ChangesetErrorEnum::NoVectorIndex {
            predicate: predicate.to_string(),
        })
    }
}
