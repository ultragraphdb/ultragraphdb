/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Where the compute binary runs on one host, declared once for whatever starts it: a test harness, a profile.

/// The binary's name.
pub const BINARY: &str = "ultragraphdb_compute";

/// The address its `/health` and `/ready` listen at on one host, which no other role's declares.
pub const OBSERVABILITY: &str = "127.0.0.1:18082";

/// The address the frontend connects back to on one host.
pub const ROLE_ADDRESS: &str = "127.0.0.1:17082";

/// The environment that starts it on one host, as `ULTRAGRAPHDB_<TABLE>_<KEY>` variables, joining the frontend. The role
/// secret's file is the starter's: a deployment's own, which no specification names.
#[must_use]
pub fn settings() -> Vec<(String, String)> {
    [
        (
            "ULTRAGRAPHDB_OBSERVABILITY_ADDRESS",
            format!("tcp://{OBSERVABILITY}"),
        ),
        (
            "ULTRAGRAPHDB_CLUSTER_ID",
            ultragraphdb_frontend_specs::CLUSTER.to_string(),
        ),
        (
            "ULTRAGRAPHDB_CLUSTER_FRONTEND_ADDRESS",
            format!("tcp://{}", ultragraphdb_frontend_specs::ROLE_ADDRESS),
        ),
        (
            "ULTRAGRAPHDB_COMPUTE_ROLE_ADDRESS",
            format!("tcp://{ROLE_ADDRESS}"),
        ),
    ]
    .map(|(name, value)| (name.to_string(), value))
    .to_vec()
}
