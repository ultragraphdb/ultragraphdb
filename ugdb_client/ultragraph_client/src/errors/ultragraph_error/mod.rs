/*
 * Copyright (c) "2026" . Marvin Hansen All Rights Reserved.
 */

mod ultragraph_error_display;
mod ultragraph_error_error;
mod ultragraph_error_from;

use tonic::Code;

use crate::errors::auth_error::AuthError;
use crate::errors::connect_error::ConnectError;
use crate::errors::connection_string_error::ConnectionStringError;
use crate::errors::transaction_error::TransactionError;

/// The error type returned by every fallible operation on this crate's public API.
///
/// The wrapped classification is private; branch on [`UltraGraphError::kind`].
#[derive(Debug, Clone, PartialEq)]
pub struct UltraGraphError(UltraGraphErrorEnum);

/// Detailed classification of Dgraph client errors.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum UltraGraphErrorEnum {
    /// A `dgraph://` connection string could not be parsed or validated.
    ConnectionString(ConnectionStringError),
    /// A connection could not be established or validated.
    Connect(ConnectError),
    /// Login or token refresh failed.
    Auth(AuthError),
    /// A transaction operation failed.
    Transaction(TransactionError),
    /// An RPC failed for a reason with no more specific classification.
    Rpc { code: Code, message: String },
}

impl UltraGraphError {
    pub(crate) fn new(variant: UltraGraphErrorEnum) -> Self {
        Self(variant)
    }

    /// Classification, for callers that need to branch on the failure.
    pub fn kind(&self) -> &UltraGraphErrorEnum {
        &self.0
    }

    /// Whether the failure was an aborted transaction, which the caller may wish to retry
    /// in a fresh transaction.
    pub fn is_aborted(&self) -> bool {
        match &self.0 {
            UltraGraphErrorEnum::Transaction(err) => err.is_aborted(),
            _ => false,
        }
    }

    /// Whether the failure was "cluster is still starting", which the caller may retry.
    pub fn is_cluster_not_ready(&self) -> bool {
        match &self.0 {
            UltraGraphErrorEnum::Connect(err) => err.is_cluster_not_ready(),
            _ => false,
        }
    }

    /// Whether the failure was a transport failure rather than a rejection by the server,
    /// so the caller may retry.
    ///
    /// An RPC counts as one in two cases:
    ///
    /// - `Unavailable`, gRPC's own "could not reach the server".
    /// - `Unknown` carrying the message `transport error`. This is not an arbitrary string
    ///   match: when a connection breaks mid-call the RPC never receives a gRPC status at
    ///   all, and tonic synthesises exactly this status to stand in for the one that never
    ///   arrived. Translating it here, once, is what keeps callers from having to match on
    ///   message text themselves -- the same reasoning that gives "Please retry" its own
    ///   typed variant.
    pub fn is_transport(&self) -> bool {
        match &self.0 {
            UltraGraphErrorEnum::Connect(err) => err.is_transport(),
            UltraGraphErrorEnum::Rpc { code, message } => {
                *code == Code::Unavailable
                    || (*code == Code::Unknown && message.contains("transport error"))
            }
            _ => false,
        }
    }

    #[allow(non_snake_case)]
    pub fn ConnectionString(err: ConnectionStringError) -> Self {
        Self::new(UltraGraphErrorEnum::ConnectionString(err))
    }

    #[allow(non_snake_case)]
    pub fn Connect(err: ConnectError) -> Self {
        Self::new(UltraGraphErrorEnum::Connect(err))
    }

    #[allow(non_snake_case)]
    pub fn Auth(err: AuthError) -> Self {
        Self::new(UltraGraphErrorEnum::Auth(err))
    }

    #[allow(non_snake_case)]
    pub fn Transaction(err: TransactionError) -> Self {
        Self::new(UltraGraphErrorEnum::Transaction(err))
    }

    #[allow(non_snake_case)]
    pub fn Rpc(code: Code, message: String) -> Self {
        Self::new(UltraGraphErrorEnum::Rpc { code, message })
    }
}
