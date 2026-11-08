/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod store_error_display;
mod store_error_error;

/// Why a store operation failed. `Display` gives Dgraph's or Badger's text where they have one: `Transaction is
/// too old` (Dgraph's `posting.ErrTsTooOld`), `Key with size … exceeded … limit` (Badger's `exceedsSize`) and `Key
/// cannot be empty` (Badger's `ErrEmptyKey`).
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StoreError(pub StoreErrorEnum);

/// The classification of a [`StoreError`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StoreErrorEnum {
    /// A read below the discard timestamp, whose versions compaction may already have collected: Dgraph's
    /// `ErrTsTooOld`, `Transaction is too old`.
    TsTooOld,
    /// A write whose key is longer than `limit` bytes, and its batch wrote nothing. `size` is the length that
    /// broke the limit: the user key's against Badger's 65,000 bytes, or the stored key's, escaped and versioned,
    /// against fjall's 65,535. `head` holds the first 1,024 bytes of that key, which the text dumps as Badger
    /// does.
    KeyTooLarge {
        /// The length that broke the limit.
        size: usize,
        /// The longest key the store takes.
        limit: usize,
        /// The first 1,024 bytes of the key.
        head: Vec<u8>,
    },
    /// A read of `key` failed, at once or partway through a scan, as fjall reports a block checksum failure.
    ReadFailed {
        /// The key read.
        key: Vec<u8>,
        /// Why the read failed, as the store reports it.
        reason: String,
    },
    /// The stored value of `key` cannot be decoded; no value is returned.
    Corrupted {
        /// The key whose value is corrupted.
        key: Vec<u8>,
        /// Why the value cannot be decoded.
        reason: String,
    },
    /// A write failed: a batch, a sync, a drop or a compaction. The store is poisoned from then on and recovers
    /// only by reopening.
    WriteFailed(String),
    /// A write refused because an earlier one failed: the store recovers only by reopening.
    Poisoned,
    /// The store could not be opened: its directory is held by another process or unreadable, or its recovery
    /// failed.
    OpenFailed(String),
    /// The store does not offer the operation named: what `ultragraphdb_traits_store`'s `Store` answers for its
    /// temporary keyspaces and rebuild records when an implementation keeps none.
    Unsupported(&'static str),
    /// A temporary keyspace by this number does not exist: it was dropped, or the store was reopened since it was
    /// created, which drops every temporary keyspace.
    UnknownTemporary(u64),
    /// A versionless write whose key is empty, and its batch wrote nothing: Badger's `ErrEmptyKey`, `Key cannot be
    /// empty`.
    EmptyKey,
}

impl StoreError {
    /// A read below the discard timestamp.
    #[must_use]
    pub fn ts_too_old() -> Self {
        Self(StoreErrorEnum::TsTooOld)
    }

    /// A write whose key of `size` bytes is longer than `limit`; `head` holds the key's first 1,024 bytes.
    #[must_use]
    pub fn key_too_large(size: usize, limit: usize, head: Vec<u8>) -> Self {
        Self(StoreErrorEnum::KeyTooLarge { size, limit, head })
    }

    /// A read of `key` failed, for `reason`.
    #[must_use]
    pub fn read_failed(key: Vec<u8>, reason: String) -> Self {
        Self(StoreErrorEnum::ReadFailed { key, reason })
    }

    /// The stored value of `key` cannot be decoded, for `reason`.
    #[must_use]
    pub fn corrupted(key: Vec<u8>, reason: String) -> Self {
        Self(StoreErrorEnum::Corrupted { key, reason })
    }

    /// A write failed, for `reason`; the store is poisoned from then on.
    #[must_use]
    pub fn write_failed(reason: String) -> Self {
        Self(StoreErrorEnum::WriteFailed(reason))
    }

    /// A write refused because an earlier one failed.
    #[must_use]
    pub fn poisoned() -> Self {
        Self(StoreErrorEnum::Poisoned)
    }

    /// The store could not be opened, for `reason`.
    #[must_use]
    pub fn open_failed(reason: String) -> Self {
        Self(StoreErrorEnum::OpenFailed(reason))
    }

    /// The store does not offer `operation`.
    #[must_use]
    pub fn unsupported(operation: &'static str) -> Self {
        Self(StoreErrorEnum::Unsupported(operation))
    }

    /// No temporary keyspace numbered `id` exists.
    #[must_use]
    pub fn unknown_temporary(id: u64) -> Self {
        Self(StoreErrorEnum::UnknownTemporary(id))
    }

    /// A versionless write whose key is empty.
    #[must_use]
    pub fn empty_key() -> Self {
        Self(StoreErrorEnum::EmptyKey)
    }
}
