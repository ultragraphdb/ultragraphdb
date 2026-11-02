/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod posting_error_display;
mod posting_error_error;
mod posting_error_from;

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
    /// A key of no list keyspace handed to the posting lists.
    NotAList {
        /// The key handed in.
        key: Vec<u8>,
    },
    /// A split part's key read as a posting list: Dgraph's `ErrInvalidKey`.
    InvalidKey,
    /// A read below the timestamp a list can answer: Dgraph's `ErrTsTooOld`.
    TsTooOld,
    /// A read below the list's complete version.
    ReadBelowMinTs {
        /// The timestamp read at.
        read_ts: u64,
        /// The list's complete version.
        min_ts: u64,
        /// The list's key.
        key: Vec<u8>,
    },
    /// A delete-all marker below the list's complete version.
    DeleteBelowMinTs {
        /// The marker's timestamp.
        delete_below_ts: u64,
        /// The list's complete version.
        min_ts: u64,
    },
    /// A schema or type entry read as a posting list.
    SchemaPosting {
        /// The entry's key.
        key: Vec<u8>,
    },
    /// A version whose meta byte is none of Dgraph's posting kinds.
    UnexpectedMeta {
        /// The meta byte read.
        meta: u8,
        /// The list's key.
        key: Vec<u8>,
    },
    /// A part a split list records is not in the store.
    MissingPart {
        /// The missing part's key.
        key: Vec<u8>,
    },
    /// A rollup cancelled because rollups are paused: Dgraph's `ErrHighPriorityOp`.
    HighPriorityOp,
    /// A commit version of a round at or below the version before it: the watermark for the round's first, the
    /// round's previous version for the others.
    StaleVersion {
        /// The stale commit version.
        version: u64,
        /// The version it should have followed.
        after: u64,
    },
}

impl PostingError {
    /// The store's `error`.
    pub fn store(error: StoreError) -> Self {
        Self(PostingErrorEnum::Store(error))
    }

    /// A `key` of no list keyspace.
    pub fn not_a_list(key: &[u8]) -> Self {
        Self(PostingErrorEnum::NotAList { key: key.to_vec() })
    }

    /// A split part's key read as a posting list.
    pub fn invalid_key() -> Self {
        Self(PostingErrorEnum::InvalidKey)
    }

    /// A read below the timestamp a list can answer.
    pub fn ts_too_old() -> Self {
        Self(PostingErrorEnum::TsTooOld)
    }

    /// A read at `read_ts` below `min_ts`, the complete version of the list at `key`.
    pub fn read_below_min_ts(read_ts: u64, min_ts: u64, key: Vec<u8>) -> Self {
        Self(PostingErrorEnum::ReadBelowMinTs {
            read_ts,
            min_ts,
            key,
        })
    }

    /// A delete-all marker at `delete_below_ts` below `min_ts`, the list's complete version.
    pub fn delete_below_min_ts(delete_below_ts: u64, min_ts: u64) -> Self {
        Self(PostingErrorEnum::DeleteBelowMinTs {
            delete_below_ts,
            min_ts,
        })
    }

    /// A schema or type entry at `key` read as a posting list.
    pub fn schema_posting(key: Vec<u8>) -> Self {
        Self(PostingErrorEnum::SchemaPosting { key })
    }

    /// A version of the list at `key` whose meta byte `meta` is no posting kind.
    pub fn unexpected_meta(meta: u8, key: Vec<u8>) -> Self {
        Self(PostingErrorEnum::UnexpectedMeta { meta, key })
    }

    /// A part at `key` a split list records and the store lacks.
    pub fn missing_part(key: Vec<u8>) -> Self {
        Self(PostingErrorEnum::MissingPart { key })
    }

    /// A rollup cancelled because rollups are paused.
    pub fn high_priority_op() -> Self {
        Self(PostingErrorEnum::HighPriorityOp)
    }

    /// A commit `version` at or below `after`, the version before it.
    pub fn stale_version(version: u64, after: u64) -> Self {
        Self(PostingErrorEnum::StaleVersion { version, after })
    }

    /// A stored value of `key` the decoder refuses, as the store's `Corrupted`.
    pub fn corrupted(key: &[u8], reason: impl Into<String>) -> Self {
        Self::store(StoreError::corrupted(key.to_vec(), reason.into()))
    }
}
