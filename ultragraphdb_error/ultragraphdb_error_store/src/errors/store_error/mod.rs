/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod store_error_display;
mod store_error_error;

/// Why a store operation failed. `Display` gives Dgraph's or Badger's text where they have one: `Transaction is
/// too old` (Dgraph's `posting.ErrTsTooOld`) and `Key with size … exceeded … limit` (Badger's `exceedsSize`).
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
        size: usize,
        limit: usize,
        head: Vec<u8>,
    },
    /// A read of `key` failed, at once or partway through a scan, as fjall reports a block checksum failure.
    ReadFailed { key: Vec<u8>, reason: String },
    /// The stored value of `key` cannot be decoded; no value is returned.
    Corrupted { key: Vec<u8>, reason: String },
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
}

impl StoreError {
    #[allow(non_snake_case)]
    pub fn TsTooOld() -> Self {
        Self(StoreErrorEnum::TsTooOld)
    }

    #[allow(non_snake_case)]
    pub fn KeyTooLarge(size: usize, limit: usize, head: Vec<u8>) -> Self {
        Self(StoreErrorEnum::KeyTooLarge { size, limit, head })
    }

    #[allow(non_snake_case)]
    pub fn ReadFailed(key: Vec<u8>, reason: String) -> Self {
        Self(StoreErrorEnum::ReadFailed { key, reason })
    }

    #[allow(non_snake_case)]
    pub fn Corrupted(key: Vec<u8>, reason: String) -> Self {
        Self(StoreErrorEnum::Corrupted { key, reason })
    }

    #[allow(non_snake_case)]
    pub fn WriteFailed(reason: String) -> Self {
        Self(StoreErrorEnum::WriteFailed(reason))
    }

    #[allow(non_snake_case)]
    pub fn Poisoned() -> Self {
        Self(StoreErrorEnum::Poisoned)
    }

    #[allow(non_snake_case)]
    pub fn OpenFailed(reason: String) -> Self {
        Self(StoreErrorEnum::OpenFailed(reason))
    }

    #[allow(non_snake_case)]
    pub fn Unsupported(operation: &'static str) -> Self {
        Self(StoreErrorEnum::Unsupported(operation))
    }

    #[allow(non_snake_case)]
    pub fn UnknownTemporary(id: u64) -> Self {
        Self(StoreErrorEnum::UnknownTemporary(id))
    }
}
