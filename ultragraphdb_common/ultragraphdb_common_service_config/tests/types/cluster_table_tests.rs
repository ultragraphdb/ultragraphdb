/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The `[cluster]` table's `system_address` (design D2): the corner cases C2.8 and C2.9 of
//! `openspec/changes/stage-5-8-system-group/notes/corner-cases.md`, as the settings read them. Every expected value is
//! the text the test writes.

use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_tool_bdd::{Context, TestResult};

/// The table each role's own file holds beside `[cluster]`.
const ROLE_TABLES: [&str; 3] = [
    "[endpoint]\nplaintext = true\n",
    "[compute]\nrole_address = \"tcp://compute-0:7300\"\n",
    "[storage]\nrole_address = \"tcp://storage-0:7200\"\ndata_dir = \"/data/storage\"\n",
];

/// C2.8: `cluster.system_address` reads as written in a frontend's, a compute instance's and a storage instance's file,
/// on either carrier.
#[test]
fn test_the_system_address_reads_in_every_role_file_on_either_carrier() -> TestResult {
    for table in ROLE_TABLES {
        for address in ["tcp://storage-0:7100", "unix:///run/ultragraph/system.sock"] {
            let text = format!("[cluster]\nsystem_address = \"{address}\"\n{table}");
            let config = ServiceConfig::parse(text.as_bytes())?;
            assert_eq!(config.cluster().system_address(), Some(address), "{text}");
        }
    }
    Ok(())
}

/// C2.9, since the switch (design D2): a file holding `cluster.frontend_address` beside `cluster.system_address` is refused,
/// the old setting naming itself.
#[test]
fn test_the_frontend_address_beside_the_system_address_is_refused() -> TestResult {
    let text = "[cluster]\nfrontend_address = \"tcp://frontend-0:7081\"\nsystem_address = \"tcp://storage-0:7100\"\n";
    let refused = ServiceConfig::parse(text.as_bytes())
        .err()
        .context("a refusal")?;
    assert!(
        refused.to_string().contains("frontend_address"),
        "{refused}"
    );
    Ok(())
}

/// A file naming no `cluster.system_address` reads it as absent: the crate invents no default.
#[test]
fn test_an_absent_system_address_reads_as_absent() -> TestResult {
    let config = ServiceConfig::parse(b"[cluster]\nid = 1\n")?;
    assert_eq!(config.cluster().system_address(), None);
    Ok(())
}
