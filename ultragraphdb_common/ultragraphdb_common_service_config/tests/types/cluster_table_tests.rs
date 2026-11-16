/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The `[cluster]` table's `system_address` (design D2): the corner cases C2.8 and C2.9 of
//! `openspec/changes/stage-5-8-system-group/notes/corner-cases.md`, as the settings read them. Every expected value is
//! the text the test writes.

use ultragraphdb_common_service_config::ServiceConfig;
use ultragraphdb_tool_bdd::TestResult;

/// The table each role's own file holds beside `[cluster]`.
const ROLE_TABLES: [&str; 3] = [
    "[frontend]\nrole_address = \"tcp://frontend-0:7081\"\ndata_dir = \"/data/frontend\"\n",
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

/// C2.9: a file holding `cluster.frontend_address` and `cluster.system_address` both reads each as written; the old
/// setting leaves in group 7.
#[test]
fn test_the_frontend_and_system_addresses_read_side_by_side() -> TestResult {
    let text = "[cluster]\nfrontend_address = \"tcp://frontend-0:7081\"\nsystem_address = \"tcp://storage-0:7100\"\n";
    let config = ServiceConfig::parse(text.as_bytes())?;
    assert_eq!(
        config.cluster().frontend_address(),
        Some("tcp://frontend-0:7081")
    );
    assert_eq!(
        config.cluster().system_address(),
        Some("tcp://storage-0:7100")
    );
    Ok(())
}

/// A file naming no `cluster.system_address` reads it as absent: the crate invents no default.
#[test]
fn test_an_absent_system_address_reads_as_absent() -> TestResult {
    let config =
        ServiceConfig::parse(b"[cluster]\nfrontend_address = \"tcp://frontend-0:7081\"\n")?;
    assert_eq!(config.cluster().system_address(), None);
    Ok(())
}
