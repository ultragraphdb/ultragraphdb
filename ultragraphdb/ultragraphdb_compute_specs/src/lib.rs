/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Where the compute binary runs on one host, declared once for whatever starts it: a test harness, a profile.

/// The binary's name.
pub const BINARY: &str = "ultragraphdb_compute";

/// The address its `/health` and `/ready` listen at on one host, which no other role's declares.
pub const OBSERVABILITY: &str = "127.0.0.1:18082";

/// The environment that starts it on one host, as `ULTRAGRAPHDB_<TABLE>_<KEY>` variables.
#[must_use]
pub fn settings() -> Vec<(String, String)> {
    vec![(
        "ULTRAGRAPHDB_OBSERVABILITY_ADDRESS".to_string(),
        format!("tcp://{OBSERVABILITY}"),
    )]
}
