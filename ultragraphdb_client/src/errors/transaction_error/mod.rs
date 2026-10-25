/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod transaction_error_display;
mod transaction_error_error;

use tonic::Code;

use crate::types::response::Response;

/// Failure of a transaction operation.
///
/// The classification is the public field `.0`; branch on it.
///
/// Note this type derives `PartialEq` but not `Eq`/`Hash`, because
/// [`TransactionErrorEnum::StartTsMismatch`] retains the response that accompanied the
/// conflict and a response payload is not hashable.
#[derive(Debug, Clone, PartialEq)]
pub struct TransactionError(pub TransactionErrorEnum);

/// Detailed classification of transaction errors.
#[derive(Debug, Clone, PartialEq)]
pub enum TransactionErrorEnum {
    /// The transaction has already been committed or discarded, or was poisoned by a
    /// failed mutation. No further operations are possible.
    Finished,
    /// A mutation or commit was attempted on a read-only transaction.
    ///
    /// The typed API makes this unreachable; it can only arise through the raw request
    /// path, where the caller supplies a request whose mutations are not visible to the
    /// type system.
    ReadOnly,
    /// The server aborted the transaction, typically because a concurrent transaction
    /// modified the same data. Retrying is the caller's decision.
    ///
    /// Unlike the Go client, the originating status code and message are retained.
    Aborted { code: Code, message: String },
    /// The server returned a start timestamp that conflicts with the one already
    /// established for this transaction, which indicates client/server desync.
    ///
    /// The accompanying response is retained because the Go client returns both a valid
    /// response and this error, and a `Result` would otherwise discard it.
    StartTsMismatch {
        expected: u64,
        found: u64,
        response: Box<Response>,
    },
}

impl TransactionError {
    pub(crate) fn new(variant: TransactionErrorEnum) -> Self {
        Self(variant)
    }

    /// Whether this transaction was aborted and may be worth retrying in a new
    /// transaction. Provided so callers never need to match on message text.
    pub fn is_aborted(&self) -> bool {
        matches!(self.0, TransactionErrorEnum::Aborted { .. })
    }

    #[allow(non_snake_case)]
    pub fn Finished() -> Self {
        Self::new(TransactionErrorEnum::Finished)
    }

    #[allow(non_snake_case)]
    pub fn ReadOnly() -> Self {
        Self::new(TransactionErrorEnum::ReadOnly)
    }

    #[allow(non_snake_case)]
    pub fn Aborted(code: Code, message: String) -> Self {
        Self::new(TransactionErrorEnum::Aborted { code, message })
    }

    #[allow(non_snake_case)]
    pub fn StartTsMismatch(expected: u64, found: u64, response: Response) -> Self {
        Self::new(TransactionErrorEnum::StartTsMismatch {
            expected,
            found,
            response: Box::new(response),
        })
    }
}
