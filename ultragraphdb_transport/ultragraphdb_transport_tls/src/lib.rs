/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! TLS with ring as rustls's one crypto provider: [`TlsTrust`], the certificates a connection is verified against,
//! read from the image's CA bundle or a PEM source; [`TlsConnector`], which opens a client connection verified against
//! them; and [`TlsAcceptor`], which accepts TLS 1.3 connections presenting a server's certificate chain and key. Each
//! connection is a [`TlsSession`] with no I/O of its own, bytes in and bytes out, so a blocking socket, the compio
//! transport and the tokio client shell drive the same code; [`TlsStream`] drives it over a blocking `std::net` stream,
//! which needs no async runtime. The trust store and the acceptor belong to the control plane, read once at startup;
//! a session's records are the data plane of the connection it carries.
//!
//! Behind the `test-utils` feature, [`TestPki`] generates a certificate authority and a server certificate it
//! issued, and serves them from a [`TestPeer`], an in-memory server in front of an application such as [`TestEcho`],
//! so tests verify real handshakes on their own thread, without a public CA or a socket; a [`TestLink`] pumps a session
//! over a [`TestPipe`] or a non-blocking socket alike.

mod errors;
mod types;
#[cfg(feature = "test-utils")]
mod utils_test;

pub use crate::errors::tls_error::{TlsError, TlsErrorEnum};
pub use crate::types::certificate_chain_file::CertificateChainFile;
pub use crate::types::plaintext::Plaintext;
pub use crate::types::private_key_file::PrivateKeyFile;
pub use crate::types::tls_acceptor::TlsAcceptor;
pub use crate::types::tls_connector::TlsConnector;
pub use crate::types::tls_session::TlsSession;
pub use crate::types::tls_stream::{
    HANDSHAKE_TURNS_MAX, READ_TURNS_MAX, RECORD_SIZE_MAX, TlsStream,
};
pub use crate::types::tls_trust::{CA_BUNDLE_KEY, DEFAULT_CA_BUNDLE, TlsTrust};
#[cfg(feature = "test-utils")]
pub use crate::utils_test::test_echo::TestEcho;
#[cfg(feature = "test-utils")]
pub use crate::utils_test::test_link::TestLink;
#[cfg(feature = "test-utils")]
pub use crate::utils_test::test_peer::TestPeer;
#[cfg(feature = "test-utils")]
pub use crate::utils_test::test_pipe::TestPipe;
#[cfg(feature = "test-utils")]
pub use crate::utils_test::test_pki::TestPki;
