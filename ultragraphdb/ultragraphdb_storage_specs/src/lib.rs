/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Where the storage binary runs on one host, declared once for whatever starts it: a test harness, a profile.

/// The binary's name.
pub const BINARY: &str = "ultragraphdb_storage";

/// The address its `/health` and `/ready` listen at on one host, which no other role's declares.
pub const OBSERVABILITY: &str = "127.0.0.1:18083";

/// The address the frontend connects back to on one host.
pub const ROLE_ADDRESS: &str = "127.0.0.1:17083";

/// The settings that start it on one host, each named `table.key`, joining the frontend. The role
/// secret's file and the data directory are the starter's: a deployment's own, which no specification names.
#[must_use]
pub fn settings() -> Vec<(String, String)> {
    [
        ("observability.address", format!("tcp://{OBSERVABILITY}")),
        (
            "cluster.id",
            ultragraphdb_frontend_specs::CLUSTER.to_string(),
        ),
        (
            "cluster.frontend_address",
            format!("tcp://{}", ultragraphdb_frontend_specs::ROLE_ADDRESS),
        ),
        ("storage.role_address", format!("tcp://{ROLE_ADDRESS}")),
        ("storage.partition_count", "1".to_string()),
    ]
    .map(|(name, value)| (name.to_string(), value))
    .to_vec()
}
