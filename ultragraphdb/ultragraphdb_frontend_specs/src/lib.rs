/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Where the frontend binary runs on one host, declared once for whatever starts it: a test harness, a profile.

/// The binary's name.
pub const BINARY: &str = "ultragraphdb_frontend";

/// The address its `/health` and `/ready` listen at on one host, which no other role's declares.
pub const OBSERVABILITY: &str = "127.0.0.1:18081";

/// The cluster a role of this host joins, which every frame's header names.
pub const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

/// The address the roles join at on one host.
pub const ROLE_ADDRESS: &str = "127.0.0.1:17081";

/// The address clients connect to on one host.
pub const CLIENT_ADDRESS: &str = "127.0.0.1:17080";

/// The environment that starts it on one host, as `ULTRAGRAPHDB_<TABLE>_<KEY>` variables, and one partition to form the
/// placement over. The role secret's file and the data directory are the starter's: a deployment's own, which no specification names.
#[must_use]
pub fn settings() -> Vec<(String, String)> {
    [
        (
            "ULTRAGRAPHDB_OBSERVABILITY_ADDRESS",
            format!("tcp://{OBSERVABILITY}"),
        ),
        ("ULTRAGRAPHDB_CLUSTER_ID", CLUSTER.to_string()),
        ("ULTRAGRAPHDB_CLUSTER_PARTITION_COUNT", "1".to_string()),
        (
            "ULTRAGRAPHDB_FRONTEND_ROLE_ADDRESS",
            format!("tcp://{ROLE_ADDRESS}"),
        ),
        (
            "ULTRAGRAPHDB_ENDPOINT_ADDRESS",
            format!("tcp://{CLIENT_ADDRESS}"),
        ),
        ("ULTRAGRAPHDB_ENDPOINT_PLAINTEXT", "true".to_string()),
        ("ULTRAGRAPHDB_SEQUENCER_EPOCH_MS_MIN", "1".to_string()),
        ("ULTRAGRAPHDB_SEQUENCER_EPOCH_MS_MAX", "20".to_string()),
    ]
    .map(|(name, value)| (name.to_string(), value))
    .to_vec()
}
