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

use ultragraphdb_core_keys::KeyError;
use ultragraphdb_error_options::OptionsError;
use ultragraphdb_store_posting::PostingError;

/// Why a vector-index operation failed. `Display` gives Dgraph's text wherever Dgraph has one: `No value found`,
/// `Key not found`, `nil vector returned`, `error fetching posting list`, `HNSW tree has no elements`, `can not compute
/// … on vectors of different lengths`, `cannot cast … to SimilarityType`, `Cannot convert … to uint slice`.
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
    /// A key could not be built.
    Key(KeyError),
    /// An option holds a value of another type.
    Options(OptionsError),
    /// A key holds no value: Dgraph's `ErrNoValue`, how a cache reports absence.
    NoValue,
    /// No key under a prefix passes the filter: Badger's `ErrKeyNotFound`, how a cache's `find` reports it.
    KeyNotFound,
    /// A key read found nothing: Dgraph's `errFetchingPostingList`, wrapping the absence and naming the key.
    FetchingPostingList {
        cause: Box<HnswError>,
        pred: String,
        uid: u64,
    },
    /// A uid holds no vector: Dgraph's `errNilVector`, wrapping the absence when a read reported one.
    NilVector(Option<Box<HnswError>>),
    /// The index holds no vector to start from: Dgraph's `EmptyHNSWTreeError`, wrapping the absence `find` reported.
    EmptyTree(Option<Box<HnswError>>),
    /// A distance between vectors of different lengths.
    DifferentLengths(&'static str),
    /// The metric option holds a value of another type.
    CannotCast(&'static str),
    /// A dead list that does not parse: Dgraph's `cannotConvertToUintSlice`.
    UintSlice(String),
    /// An index whose float width is not its vectors' type's; Dgraph panics on a width other than 32 or 64.
    FloatBits { bits: usize, expected: usize },
    /// An option below 1, which Dgraph accepts and panics on at the first search or insert that reaches it.
    InvalidParameter { name: &'static str, value: i64 },
    /// A node's neighbour lists without the level read; Dgraph panics (index out of range).
    LevelMissing {
        uid: u64,
        level: usize,
        levels: usize,
    },
    /// A layer search that kept no neighbour, with a breadth or result count of 0; Dgraph panics (index out of
    /// range).
    NoNeighbour,
}

impl HnswError {
    #[allow(non_snake_case)]
    pub fn Posting(error: PostingError) -> Self {
        Self(HnswErrorEnum::Posting(error))
    }

    #[allow(non_snake_case)]
    pub fn Key(error: KeyError) -> Self {
        Self(HnswErrorEnum::Key(error))
    }

    #[allow(non_snake_case)]
    pub fn Options(error: OptionsError) -> Self {
        Self(HnswErrorEnum::Options(error))
    }

    #[allow(non_snake_case)]
    pub fn NoValue() -> Self {
        Self(HnswErrorEnum::NoValue)
    }

    #[allow(non_snake_case)]
    pub fn KeyNotFound() -> Self {
        Self(HnswErrorEnum::KeyNotFound)
    }

    #[allow(non_snake_case)]
    pub fn FetchingPostingList(cause: HnswError, pred: impl Into<String>, uid: u64) -> Self {
        Self(HnswErrorEnum::FetchingPostingList {
            cause: Box::new(cause),
            pred: pred.into(),
            uid,
        })
    }

    #[allow(non_snake_case)]
    pub fn NilVector(cause: Option<HnswError>) -> Self {
        Self(HnswErrorEnum::NilVector(cause.map(Box::new)))
    }

    #[allow(non_snake_case)]
    pub fn EmptyTree(cause: Option<HnswError>) -> Self {
        Self(HnswErrorEnum::EmptyTree(cause.map(Box::new)))
    }

    #[allow(non_snake_case)]
    pub fn DifferentLengths(func_name: &'static str) -> Self {
        Self(HnswErrorEnum::DifferentLengths(func_name))
    }

    #[allow(non_snake_case)]
    pub fn CannotCast(go_type: &'static str) -> Self {
        Self(HnswErrorEnum::CannotCast(go_type))
    }

    #[allow(non_snake_case)]
    pub fn UintSlice(s: impl Into<String>) -> Self {
        Self(HnswErrorEnum::UintSlice(s.into()))
    }

    #[allow(non_snake_case)]
    pub fn FloatBits(bits: usize, expected: usize) -> Self {
        Self(HnswErrorEnum::FloatBits { bits, expected })
    }

    #[allow(non_snake_case)]
    pub fn InvalidParameter(name: &'static str, value: i64) -> Self {
        Self(HnswErrorEnum::InvalidParameter { name, value })
    }

    #[allow(non_snake_case)]
    pub fn LevelMissing(uid: u64, level: usize, levels: usize) -> Self {
        Self(HnswErrorEnum::LevelMissing { uid, level, levels })
    }

    #[allow(non_snake_case)]
    pub fn NoNeighbour() -> Self {
        Self(HnswErrorEnum::NoNeighbour)
    }

    /// A stored value of `key` the index cannot decode, as the store's `Corrupted`.
    pub fn corrupted(key: &[u8], reason: impl Into<String>) -> Self {
        Self::Posting(PostingError::corrupted(key, reason))
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
            _ => false,
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
