/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Client-side TLS over a blocking `std::net` socket, with ring as rustls's one crypto provider: [`TlsTrust`], the
//! certificates a connection is verified against, read from the image's CA bundle or a PEM source, and
//! [`TlsConnector`], which wraps an open socket in a connection verified against them. Nothing here needs an async
//! runtime. Both belong to the control plane: a trust store is read once at startup, a connection is verified once
//! when it opens.
//!
//! Behind the `test-utils` feature, [`TestPki`] generates a certificate authority and a server certificate it
//! issued, and serves them from a [`TestPeer`], an in-memory server in front of an application such as [`TestEcho`],
//! so tests verify real handshakes on their own thread, without a public CA or a socket.

mod errors;
mod types;
#[cfg(feature = "test-utils")]
mod utils_test;

pub use crate::errors::tls_error::{TlsError, TlsErrorEnum};
pub use crate::types::tls_connector::{TlsConnector, TlsStream};
pub use crate::types::tls_trust::{CA_BUNDLE_KEY, DEFAULT_CA_BUNDLE, TlsTrust};
#[cfg(feature = "test-utils")]
pub use crate::utils_test::test_echo::TestEcho;
#[cfg(feature = "test-utils")]
pub use crate::utils_test::test_peer::TestPeer;
#[cfg(feature = "test-utils")]
pub use crate::utils_test::test_pki::TestPki;
