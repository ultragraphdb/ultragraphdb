/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/helper.go (errNilVector, errFetchingPostingList, EmptyHNSWTreeError, the
 * errors of applyDistanceFunction and ParseEdges) and persistent_hnsw.go (the errors of applyOptions). Changed for
 * ultragraphdb: translated to Rust; Go's wrapped errors as variants holding their cause; the cases where Dgraph
 * panics or exits (an invalid float width, an option below 1, a stored value that does not decode, a node without
 * the level read, an empty layer result) as variants of their own.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod hnsw_error_display;
mod hnsw_error_error;
mod hnsw_error_from;

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_error_options::OptionsError;
use ultragraphdb_store_posting::PostingError;

/// Why a vector-index operation failed. `Display` gives Dgraph's text wherever Dgraph has one: `No value found`,
/// `Key not found`, `nil vector returned`, `error fetching posting list`, `HNSW tree has no elements`, `can not compute
/// … on vectors of different lengths`, `cannot cast … to SimilarityType`.
///
/// A stored value the index cannot decode is the posting crate's corruption, the store's `Corrupted` naming the key,
/// so a caller sees one kind of corruption whichever decoder found it.
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HnswError(pub HnswErrorEnum);

/// The classification of an [`HnswError`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HnswErrorEnum {
    /// A read of the posting lists failed, or a stored value is corrupted.
    Posting(PostingError),
    /// An option holds a value of another type.
    Options(OptionsError),
    /// A key holds no value: Dgraph's `ErrNoValue`, how a cache reports absence.
    NoValue,
    /// No key under a prefix passes the filter: Badger's `ErrKeyNotFound`, how a cache's `find` reports it.
    KeyNotFound,
    /// A key read found nothing: Dgraph's `errFetchingPostingList`, wrapping the absence and naming the key.
    FetchingPostingList {
        /// The absence the read reported.
        cause: Box<HnswError>,
        /// The predicate of the key read.
        pred: String,
        /// The uid of the key read.
        uid: SystemId,
    },
    /// A uid holds no vector: Dgraph's `errNilVector`, wrapping the absence when a read reported one.
    NilVector(Option<Box<HnswError>>),
    /// The index holds no vector to start from: Dgraph's `EmptyHNSWTreeError`, wrapping the absence `find` reported.
    EmptyTree(Option<Box<HnswError>>),
    /// A distance between vectors of different lengths.
    DifferentLengths(&'static str),
    /// The metric option holds a value of another type.
    CannotCast(&'static str),
    /// A dead list that is not a JSON array of 32-digit hexadecimal system IDs; holds the text found.
    MalformedDeadList(String),
    /// An index whose float width is not its vectors' type's; Dgraph panics on a width other than 32 or 64.
    FloatBits {
        /// The index's float width.
        bits: usize,
        /// The width of its vectors' type.
        expected: usize,
    },
    /// An option below 1, which Dgraph accepts and panics on at the first search or insert that reaches it.
    InvalidParameter {
        /// The option's name.
        name: &'static str,
        /// The value it holds.
        value: i64,
    },
    /// A node's neighbour lists without the level read; Dgraph panics (index out of range).
    LevelMissing {
        /// The node read.
        uid: SystemId,
        /// The level the search read.
        level: usize,
        /// How many levels the node's lists hold.
        levels: usize,
    },
    /// A layer search that kept no neighbour, with a breadth or result count of 0; Dgraph panics (index out of
    /// range).
    NoNeighbour,
}

impl HnswError {
    /// A read of the posting lists that failed with `error`.
    pub fn posting(error: PostingError) -> Self {
        Self(HnswErrorEnum::Posting(error))
    }

    /// An option whose value is of another type, as `error` states.
    pub fn options(error: OptionsError) -> Self {
        Self(HnswErrorEnum::Options(error))
    }

    /// A key that holds no value.
    pub fn no_value() -> Self {
        Self(HnswErrorEnum::NoValue)
    }

    /// No key under a prefix that passes the filter.
    pub fn key_not_found() -> Self {
        Self(HnswErrorEnum::KeyNotFound)
    }

    /// A read of the key of `pred` and `uid` that found nothing, wrapping the absence `cause`.
    pub fn fetching_posting_list(cause: HnswError, pred: impl Into<String>, uid: SystemId) -> Self {
        Self(HnswErrorEnum::FetchingPostingList {
            cause: Box::new(cause),
            pred: pred.into(),
            uid,
        })
    }

    /// A uid that holds no vector, wrapping the absence `cause` when a read reported one.
    pub fn nil_vector(cause: Option<HnswError>) -> Self {
        Self(HnswErrorEnum::NilVector(cause.map(Box::new)))
    }

    /// An index with no vector to start from, wrapping the absence `cause` when `find` reported one.
    pub fn empty_tree(cause: Option<HnswError>) -> Self {
        Self(HnswErrorEnum::EmptyTree(cause.map(Box::new)))
    }

    /// A distance `func_name` between vectors of different lengths.
    pub fn different_lengths(func_name: &'static str) -> Self {
        Self(HnswErrorEnum::DifferentLengths(func_name))
    }

    /// A metric option that holds a value of another type than `go_type`.
    pub fn cannot_cast(go_type: &'static str) -> Self {
        Self(HnswErrorEnum::CannotCast(go_type))
    }

    /// A dead list `s` that does not parse.
    pub fn malformed_dead_list(s: impl Into<String>) -> Self {
        Self(HnswErrorEnum::MalformedDeadList(s.into()))
    }

    /// An index of float width `bits` whose vectors' type is `expected` bits wide.
    pub fn float_bits(bits: usize, expected: usize) -> Self {
        Self(HnswErrorEnum::FloatBits { bits, expected })
    }

    /// The option `name` holding `value`, below 1.
    pub fn invalid_parameter(name: &'static str, value: i64) -> Self {
        Self(HnswErrorEnum::InvalidParameter { name, value })
    }

    /// The lists of node `uid`, holding `levels` levels, without the level `level` read.
    pub fn level_missing(uid: SystemId, level: usize, levels: usize) -> Self {
        Self(HnswErrorEnum::LevelMissing { uid, level, levels })
    }

    /// A layer search that kept no neighbour.
    pub fn no_neighbour() -> Self {
        Self(HnswErrorEnum::NoNeighbour)
    }

    /// A stored value of `key` the index cannot decode, as the store's `Corrupted`.
    pub fn corrupted(key: &[u8], reason: impl Into<String>) -> Self {
        Self::posting(PostingError::corrupted(key, reason))
    }

    /// Whether a cache reported absence: no value under a key, or no key under a prefix.
    pub fn is_absence(&self) -> bool {
        matches!(self.0, HnswErrorEnum::NoValue | HnswErrorEnum::KeyNotFound)
    }

    /// Go's `errors.Is(err, errFetchingPostingList)`: whether the error is, or wraps, a key read that found nothing.
    pub fn is_fetching_posting_list(&self) -> bool {
        match &self.0 {
            HnswErrorEnum::FetchingPostingList { .. } => true,
            HnswErrorEnum::NilVector(Some(cause)) | HnswErrorEnum::EmptyTree(Some(cause)) => {
                cause.is_fetching_posting_list()
            }
            HnswErrorEnum::Posting(_)
            | HnswErrorEnum::Options(_)
            | HnswErrorEnum::NoValue
            | HnswErrorEnum::KeyNotFound
            | HnswErrorEnum::NilVector(_)
            | HnswErrorEnum::EmptyTree(_)
            | HnswErrorEnum::DifferentLengths(_)
            | HnswErrorEnum::CannotCast(_)
            | HnswErrorEnum::MalformedDeadList(_)
            | HnswErrorEnum::FloatBits { .. }
            | HnswErrorEnum::InvalidParameter { .. }
            | HnswErrorEnum::LevelMissing { .. }
            | HnswErrorEnum::NoNeighbour => false,
        }
    }

    /// Go's `errors.Is(err, errNilVector)`: whether a uid holds no vector.
    pub fn is_nil_vector(&self) -> bool {
        matches!(self.0, HnswErrorEnum::NilVector(_))
    }

    /// The empty-index result a search reports when the index holds no vector, `HNSW tree has no elements: Key not
    /// found`, which Dgraph's query treats as no neighbours (`worker/task.go`).
    pub fn is_empty_index(&self) -> bool {
        matches!(&self.0, HnswErrorEnum::EmptyTree(Some(cause)) if cause.0 == HnswErrorEnum::KeyNotFound)
    }
}
