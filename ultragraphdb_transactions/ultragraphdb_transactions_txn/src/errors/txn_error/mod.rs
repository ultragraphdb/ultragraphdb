/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod txn_error_display;
mod txn_error_error;

use ultragraphdb_store_apply::ChangesetError;

/// Why a transaction's request was refused on its worker.
///
/// - **Retryable:** [`PastLimit`](Self::PastLimit). The transaction is aborted and wrote nothing; the same work in a
///   new transaction may succeed.
/// - **Refused, not retryable:** [`ReadAboveWatermark`](Self::ReadAboveWatermark), [`WriteSetTooLarge`](Self::WriteSetTooLarge),
///   [`ReadSetTooLarge`](Self::ReadSetTooLarge), [`ReadOnly`](Self::ReadOnly), [`Namespace`](Self::Namespace),
///   [`Ended`](Self::Ended), [`Changeset`](Self::Changeset). Retrying the same request meets the same refusal.
///
/// [`TxnError::is_retryable`] gives the classification; the variant is the public field `.0`.
#[derive(Debug, Clone, PartialEq)]
pub enum TxnErrorEnum {
    /// A request named read version `version`, above the visibility watermark `watermark`: refused at once, never
    /// waited on.
    ReadAboveWatermark {
        /// The read version the request named.
        version: u64,
        /// The visibility watermark it is above.
        watermark: u64,
    },
    /// The transaction lived past its limit of `limit_ms` milliseconds, 10 s for a read-write and 16 s for a
    /// read-only one by default; it is aborted and wrote nothing.
    PastLimit {
        /// The transaction's limit, in milliseconds.
        limit_ms: u64,
    },
    /// The transaction's write-set record of `size` bytes exceeds `limit`, the maximum frame by default.
    WriteSetTooLarge {
        /// The record's size in bytes.
        size: usize,
        /// The largest record allowed, in bytes.
        limit: usize,
    },
    /// The transaction's read set would hold `size` bytes, past `limit`.
    ReadSetTooLarge {
        /// The bytes the read set would hold.
        size: usize,
        /// The most bytes it may hold.
        limit: usize,
    },
    /// A write in a read-only transaction.
    ReadOnly,
    /// A changeset of namespace `changeset` in a transaction of namespace `transaction`.
    Namespace {
        /// The transaction's namespace.
        transaction: u64,
        /// The changeset's namespace.
        changeset: u64,
    },
    /// A request of a transaction already ended by a refusal or its limit.
    Ended,
    /// The changeset was refused, or the store failed; the transaction is aborted.
    Changeset(ChangesetError),
}

/// A transaction's refusal on its worker; see [`TxnErrorEnum`].
#[derive(Debug, Clone, PartialEq)]
pub struct TxnError(pub TxnErrorEnum);

impl TxnError {
    /// A request naming read version `version`, above the watermark `watermark`.
    #[allow(non_snake_case)]
    pub fn ReadAboveWatermark(version: u64, watermark: u64) -> Self {
        Self(TxnErrorEnum::ReadAboveWatermark { version, watermark })
    }

    /// The transaction lived past its limit of `limit_ms` milliseconds.
    #[allow(non_snake_case)]
    pub fn PastLimit(limit_ms: u64) -> Self {
        Self(TxnErrorEnum::PastLimit { limit_ms })
    }

    /// A write-set record of `size` bytes, past `limit`.
    #[allow(non_snake_case)]
    pub fn WriteSetTooLarge(size: usize, limit: usize) -> Self {
        Self(TxnErrorEnum::WriteSetTooLarge { size, limit })
    }

    /// A read set of `size` bytes, past `limit`.
    #[allow(non_snake_case)]
    pub fn ReadSetTooLarge(size: usize, limit: usize) -> Self {
        Self(TxnErrorEnum::ReadSetTooLarge { size, limit })
    }

    /// A write in a read-only transaction.
    #[allow(non_snake_case)]
    pub fn ReadOnly() -> Self {
        Self(TxnErrorEnum::ReadOnly)
    }

    /// A changeset of namespace `changeset` in a transaction of namespace `transaction`.
    #[allow(non_snake_case)]
    pub fn Namespace(transaction: u64, changeset: u64) -> Self {
        Self(TxnErrorEnum::Namespace {
            transaction,
            changeset,
        })
    }

    /// A request of a transaction already ended.
    #[allow(non_snake_case)]
    pub fn Ended() -> Self {
        Self(TxnErrorEnum::Ended)
    }

    /// The changeset's refusal or the store's failure, `error`.
    #[allow(non_snake_case)]
    pub fn Changeset(error: ChangesetError) -> Self {
        Self(TxnErrorEnum::Changeset(error))
    }

    /// Whether the client may retry the transaction: only past its limit.
    pub fn is_retryable(&self) -> bool {
        matches!(self.0, TxnErrorEnum::PastLimit { .. })
    }
}

impl From<ChangesetError> for TxnError {
    fn from(error: ChangesetError) -> Self {
        Self::Changeset(error)
    }
}
