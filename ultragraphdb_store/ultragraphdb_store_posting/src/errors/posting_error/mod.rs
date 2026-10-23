/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod posting_error_display;
mod posting_error_error;
mod posting_error_from;

use ultragraphdb_core_keys::KeyError;
use ultragraphdb_error_store::StoreError;

/// Why a posting-list operation failed. `Display` gives Dgraph's text wherever Dgraph has one: `No value found`,
/// `cannot read posting list using multi-part list key`, `Transaction is too old`, and the texts of `posting`'s
/// formatted errors.
///
/// A stored value the decoder refuses is the store's [`StoreError`] `Corrupted`, naming the key, so a caller sees
/// one kind of corruption whether the store or the decoder found it.
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostingError(pub PostingErrorEnum);

/// The classification of a [`PostingError`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostingErrorEnum {
    /// The store refused or failed an operation, or a stored value is corrupted.
    Store(StoreError),
    /// A key could not be parsed or built.
    Key(KeyError),
    /// A split part's key read as a posting list: Dgraph's `ErrInvalidKey`.
    InvalidKey,
    /// No value matches: Dgraph's `ErrNoValue`.
    NoValue,
    /// A read below the timestamp a list can answer: Dgraph's `ErrTsTooOld`.
    TsTooOld,
    /// A read below the list's complete version.
    ReadBelowMinTs {
        read_ts: u64,
        min_ts: u64,
        key: Vec<u8>,
    },
    /// A delete-all marker below the list's complete version.
    DeleteBelowMinTs { delete_below_ts: u64, min_ts: u64 },
    /// A schema or type entry read as a posting list.
    SchemaPosting { key: Vec<u8> },
    /// A version whose meta byte is none of Dgraph's posting kinds.
    UnexpectedMeta { meta: u8, key: Vec<u8> },
    /// A part a split list records is not in the store.
    MissingPart { key: Vec<u8> },
    /// An edge whose operation is none of set, delete and overwrite; Dgraph exits the process (`x.Fatalf`).
    UnhandledOperation { op: i32 },
    /// A rolled-up pack whose block size is not the posting lists' 256.
    BlockSize { actual: u32, expected: u32 },
    /// A rollup cancelled because rollups are paused: Dgraph's `ErrHighPriorityOp`.
    HighPriorityOp,
}

impl PostingError {
    #[allow(non_snake_case)]
    pub fn Store(error: StoreError) -> Self {
        Self(PostingErrorEnum::Store(error))
    }

    #[allow(non_snake_case)]
    pub fn Key(error: KeyError) -> Self {
        Self(PostingErrorEnum::Key(error))
    }

    #[allow(non_snake_case)]
    pub fn InvalidKey() -> Self {
        Self(PostingErrorEnum::InvalidKey)
    }

    #[allow(non_snake_case)]
    pub fn NoValue() -> Self {
        Self(PostingErrorEnum::NoValue)
    }

    #[allow(non_snake_case)]
    pub fn TsTooOld() -> Self {
        Self(PostingErrorEnum::TsTooOld)
    }

    #[allow(non_snake_case)]
    pub fn ReadBelowMinTs(read_ts: u64, min_ts: u64, key: Vec<u8>) -> Self {
        Self(PostingErrorEnum::ReadBelowMinTs {
            read_ts,
            min_ts,
            key,
        })
    }

    #[allow(non_snake_case)]
    pub fn DeleteBelowMinTs(delete_below_ts: u64, min_ts: u64) -> Self {
        Self(PostingErrorEnum::DeleteBelowMinTs {
            delete_below_ts,
            min_ts,
        })
    }

    #[allow(non_snake_case)]
    pub fn SchemaPosting(key: Vec<u8>) -> Self {
        Self(PostingErrorEnum::SchemaPosting { key })
    }

    #[allow(non_snake_case)]
    pub fn UnexpectedMeta(meta: u8, key: Vec<u8>) -> Self {
        Self(PostingErrorEnum::UnexpectedMeta { meta, key })
    }

    #[allow(non_snake_case)]
    pub fn MissingPart(key: Vec<u8>) -> Self {
        Self(PostingErrorEnum::MissingPart { key })
    }

    #[allow(non_snake_case)]
    pub fn UnhandledOperation(op: i32) -> Self {
        Self(PostingErrorEnum::UnhandledOperation { op })
    }

    #[allow(non_snake_case)]
    pub fn BlockSize(actual: u32, expected: u32) -> Self {
        Self(PostingErrorEnum::BlockSize { actual, expected })
    }

    #[allow(non_snake_case)]
    pub fn HighPriorityOp() -> Self {
        Self(PostingErrorEnum::HighPriorityOp)
    }

    /// A stored value of `key` the decoder refuses, as the store's `Corrupted`.
    pub fn corrupted(key: &[u8], reason: impl Into<String>) -> Self {
        Self::Store(StoreError::Corrupted(key.to_vec(), reason.into()))
    }
}
