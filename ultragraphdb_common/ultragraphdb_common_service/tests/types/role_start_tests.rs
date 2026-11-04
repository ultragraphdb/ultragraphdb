/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_service::{RoleIdentity, RoleStart};
use ultragraphdb_tool_bdd::{Context, TestResult};

fn identity() -> RoleIdentity {
    RoleIdentity::new("ci", "storage_1", 7)
}

#[test]
fn test_a_start_reads_its_address_and_keeps_what_it_runs_as() -> TestResult {
    let text = "[observability]\naddress = \"tcp://127.0.0.1:18080\"\n";
    let start = RoleStart::from_settings(text.as_bytes(), identity())?;
    assert_eq!(start.observability().to_string(), "127.0.0.1:18080");
    assert_eq!(
        start.config().observability().address(),
        Some("tcp://127.0.0.1:18080")
    );
    assert_eq!(start.identity(), &identity());
    Ok(())
}

/// Each is a configuration error, naming what it refused and what it was running as.
#[test]
fn test_a_start_refuses_what_it_cannot_use() -> TestResult {
    let cases = [
        ("", "observability.address"),
        (
            "[observability]\naddress = \"127.0.0.1:18080\"\n",
            "observability.address",
        ),
        (
            "[observability]\naddress = \"tcp://nowhere\"\n",
            "observability.address",
        ),
        ("[storage]\npartiton_count = 2\n", "partiton_count"),
    ];
    for (text, named) in cases {
        let refused = RoleStart::from_settings(text.as_bytes(), identity())
            .err()
            .context("a refusal")?;
        assert_eq!(refused.exit_code(), 78, "{refused}");
        let message = refused.to_string();
        assert!(message.contains(named), "{message} names {named}");
        assert!(message.contains("instance storage_1"), "{message}");
    }
    Ok(())
}
