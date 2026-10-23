/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Shared helpers and canned responses for tests.
//!
//! In `src/`, not `tests/`: Bazel cannot reach helper files inside `tests/` but can reach all of
//! `src/` during testing. The side effect is that these helpers are themselves compiled and
//! counted. They are `pub` because integration tests are a separate crate and reach them through
//! the public API like any other consumer.

use crate::TlsMode;
use crate::utils_test::errors::fixture_error::FixtureError;
use crate::utils_test::types::endpoint::Endpoint;
use std::time::Duration;

/// How long the CA bundle fetch gets to answer.
const CA_FETCH_TIMEOUT_SECS: u64 = 10;

/// An endpoint with no Dgraph behind it, for testing this module's own logic.
///
/// The only way to build an [`Endpoint`] without reading the environment. Deliberately here
/// rather than as a public constructor, so production callers have exactly one path
/// ([`Endpoint::from_env`]) and cannot restate an endpoint by hand.
pub fn endpoint(host: &str, port: u16, tls_mode: TlsMode) -> Endpoint {
    Endpoint::new(host, port, tls_mode)
}

/// Fetch the PEM bundle published at `url`.
///
/// Fetched rather than read from a stored copy: a certificate authority is rotated, and a copy
/// is correct until it silently is not.
///
/// VERIFICATION IS ON for this request, unlike the health check. The bundle is the thing the
/// client will later trust, so taking it from an unauthenticated connection would make trusting
/// it meaningless. A health check has no such property to protect -- it carries no secret and
/// establishes nothing.
pub fn fetch_ca_bundle(url: &str) -> Result<String, FixtureError> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(CA_FETCH_TIMEOUT_SECS)))
        .build()
        .into();

    let body = agent
        .get(url)
        .call()
        .map_err(|err| FixtureError::health(url, err.to_string()))?
        .body_mut()
        .read_to_string()
        .map_err(|err| FixtureError::health(url, format!("reading the response: {err}")))?;

    // A PEM header, not merely a 200. A proxy or a login page answers 200 with HTML, and the
    // failure would otherwise surface much later as an unreadable trust store.
    if !body.contains("-----BEGIN CERTIFICATE-----") {
        return Err(FixtureError::health(
            url,
            "the response contains no PEM certificate".to_string(),
        ));
    }

    Ok(body)
}

/// A `/health?all` body with one zero and one alpha, both healthy, as v25.4.0 renders it.
pub const HEALTHY_BODY: &str = r#"[
  {"instance":"zero","address":"z0:5080","status":"healthy","group":"0","version":"v25.4.0","uptime":973,"lastEcho":1787196174},
  {"instance":"alpha","address":"a0:7080","status":"healthy","group":"1","version":"v25.4.0","uptime":38,"lastEcho":1787196175,"ongoing":["opRollup"],"ee_features":["backup_restore","cdc"],"max_assigned":4}
]"#;

/// The same, with the alpha still coming up.
pub const UNHEALTHY_BODY: &str = r#"[
  {"instance":"zero","address":"z0:5080","status":"healthy","group":"0","version":"v25.4.0"},
  {"instance":"alpha","address":"a0:7080","status":"unhealthy","group":"1","version":"v25.4.0"}
]"#;
