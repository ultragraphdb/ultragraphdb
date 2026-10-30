/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! SCRAM-SHA-256 (RFC 5802, RFC 7677) for the binary protocol's Authenticate exchange (`PROTOCOL.md` §5.3,
//! `SECURITY.md`), sans-I/O: the client's and the server's messages, their parsing and the proofs, with the
//! randomness each side's nonce takes passed in, which [`system_nonce`] draws from the operating system. A password
//! is prepared with SASLprep (RFC 4013) before its key is derived, so a client in any language derives the same key
//! from the same password. Without channel binding: the client's GS2 header is `n,,`.
//!
//! [`ScramClient`] is the client's mechanism, which a client's [`ScramLogin`] starts afresh for each exchange, and
//! [`ScramAuthenticator`] the server's: the transport's [`ultragraphdb_protocol_transport::ClientMechanism`],
//! [`ultragraphdb_protocol_transport::Login`] and [`ultragraphdb_protocol_transport::Authenticator`];
//! [`ScramVerifier`] is what a server stores of a password: its salt, its iteration count and the two keys derived
//! from it, never the password. A name no principal holds is answered with a verifier derived from the server's
//! secret, so the exchange fails as a wrong password's does, after the same work.

mod errors;
mod traits;
mod types;
mod utils;

pub use crate::errors::scram_error::{ScramError, ScramErrorEnum};
pub use crate::traits::credentials::Credentials;
pub use crate::types::scram_authenticator::ScramAuthenticator;
pub use crate::types::scram_client::ScramClient;
pub use crate::types::scram_exchange::ScramExchange;
pub use crate::types::scram_login::ScramLogin;
pub use crate::types::scram_server::ScramServer;
pub use crate::types::scram_verifier::ScramVerifier;
pub use crate::utils::constants::{DEFAULT_ITERATIONS, MECHANISM, MIN_ITERATIONS, SALT_SIZE};
pub use crate::utils::nonce::{NONCE_BYTES, system_nonce};
