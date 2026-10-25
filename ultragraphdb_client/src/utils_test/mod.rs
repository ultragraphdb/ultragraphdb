/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Obtain a running Dgraph for a test, wherever the test happens to run.
//!
//! ```no_run
//! # #[cfg(feature = "test-utils")]
//! # fn main() -> Result<(), ultragraphdb_client::utils_test::FixtureError> {
//! use ultragraphdb_client::utils_test::DgraphInstance;
//!
//! let dgraph = DgraphInstance::acquire()?;
//! // dgraph.connection_string() -> "dgraph://host:port[?sslmode=...]"
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "test-utils"))]
//! # fn main() {}
//! ```
//!
//! # Behind the `test-utils` feature, and off by default
//!
//! Acquiring a Dgraph means talking to a Docker daemon and to an HTTP health endpoint, which is
//! a Docker client, an HTTP agent and a JSON parser. A consumer of [`crate::UltraGraphClient`] has
//! no use for any of them, so they are optional dependencies and this module is compiled only
//! when the feature asks for it.
//!
//! # The environment is the whole input
//!
//! [`STRATEGY_ENV`] decides how a Dgraph is obtained -- `container` provisions a
//! `dgraph/standalone`, `existing` verifies one this process did not create -- and
//! [`HOST_ENV`], [`PORT_ENV`] and [`SSLMODE_ENV`] say where it is and what transport security
//! it expects. Every one has a default, so a workstation with Docker running needs no
//! configuration at all.
//!
//! Deliberately NOT a Docker probe and NOT a `CI=true` variable. Detecting Docker answers "can
//! I start a container", never "may I" -- a workstation with a daemon running and a shared
//! cluster configured would happily start one anyway.
//!
//! # This module does not connect to Dgraph
//!
//! Readiness is decided over Dgraph's HTTP `/health` endpoint, so nothing here uses the gRPC
//! client and nothing here needs a CA: a liveness check asks whether something is serving, not
//! who it is. Establishing a verified session is [`crate::UltraGraphClient`]'s job.
//!
//! # Ownership is part of the answer
//!
//! [`DgraphInstance::exclusivity`] reports whether this process created the instance. A
//! container it started is [`Exclusivity::Exclusive`] and may be wiped; a pre-existing Dgraph is
//! [`Exclusivity::Shared`] and must not be. Callers that destroy data are expected to branch on
//! it, because nothing else can tell them apart from the endpoint alone.

pub mod errors;
pub mod fixtures;
pub mod traits;
pub mod types;

pub use crate::utils_test::errors::fixture_error::{FixtureError, FixtureErrorEnum};
pub use crate::utils_test::traits::instance_provider::InstanceProvider;
pub use crate::utils_test::types::container_provider::ContainerProvider;
pub use crate::utils_test::types::dgraph_instance::DgraphInstance;
pub use crate::utils_test::types::endpoint::Endpoint;
pub use crate::utils_test::types::exclusivity::Exclusivity;
pub use crate::utils_test::types::existing_provider::ExistingProvider;
pub use crate::utils_test::types::health_report::{HealthReport, ServerHealth};
pub use crate::utils_test::types::run_id::RunId;
pub use crate::utils_test::types::strategy::Strategy;

/// How a Dgraph is obtained: `container` or `existing`. Defaults to `container`.
pub const STRATEGY_ENV: &str = "DGRAPH_TEST_STRATEGY";

/// Where Dgraph is. Defaults to [`DEFAULT_HOST`].
pub const HOST_ENV: &str = "DGRAPH_TEST_HOST";

/// Dgraph's gRPC port. Defaults to [`DEFAULT_PORT`].
pub const PORT_ENV: &str = "DGRAPH_TEST_PORT";

/// Transport security, in the same spelling a connection string uses: `disable`, `require` or
/// `verify-ca`. Defaults to `disable`.
pub const SSLMODE_ENV: &str = "DGRAPH_TEST_SSLMODE";

/// Where this deployment publishes the CA that verifies Dgraph, when it publishes one. Optional.
pub const CA_BUNDLE_URL_ENV: &str = "DGRAPH_TEST_CA_BUNDLE_URL";

/// This run's correlation id. Optional; see [`RunId`].
pub const RUN_ID_ENV: &str = "DGRAPH_TEST_RUN_ID";

/// The ACL password for `groot` on a Dgraph this process did not create. Required only by the
/// `existing` strategy -- a container this module started uses [`DEFAULT_GROOT_PASSWORD`].
pub const ADMIN_PASSWORD_ENV: &str = "DGRAPH_ADMIN_PASSWORD";

/// The loopback address, which is where both a host-networked container and a CI service
/// container serve.
pub const DEFAULT_HOST: &str = "127.0.0.1";

/// Dgraph's gRPC port, fixed by Dgraph.
pub const DEFAULT_PORT: u16 = 9080;

/// Dgraph's initial password for `groot`, in namespace 0 and in every namespace created
/// afterwards.
///
/// A PROPERTY OF DGRAPH, not a secret. `CreateNamespaceRequest` carries no password field and
/// the gRPC API has no user-management RPC, so a new namespace's guardian starts here and
/// nothing can choose otherwise. Published in Dgraph's own documentation, which is what makes
/// storing it in a secret store misleading rather than careful.
///
/// A container this module started uses it directly: the container is disposable, local, and
/// its credential was never anybody's to choose. A deployed cluster's namespace-0 password IS
/// chosen, and is read from [`ADMIN_PASSWORD_ENV`] instead.
pub const DEFAULT_GROOT_PASSWORD: &str = "password";

/// Read an environment variable, treating an empty value as unset.
///
/// The distinction matters because CI systems routinely export a variable with no value for an
/// unfilled secret, and `""` is not a hostname.
pub(crate) fn env_var(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => Some(value.trim().to_string()),
        _ => None,
    }
}
