/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_role::RoleCluster;
use ultragraphdb_common_service::{RoleIdentity, RoleStart};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_tool_bdd::{Context, TestResult};

/// A start holding `settings`, each a line of the `[cluster]` table.
fn start(settings: &[String]) -> TestResult<RoleStart> {
    let text = format!(
        "[observability]\naddress = \"tcp://127.0.0.1:1\"\n[cluster]\n{}\n",
        settings.join("\n")
    );
    Ok(RoleStart::from_settings(
        text.as_bytes(),
        RoleIdentity::new("ci", "storage", 1),
    )?)
}

fn secret_file(name: &str) -> TestResult<String> {
    let file = std::env::temp_dir().join(format!("role-cluster-{name}-{}", std::process::id()));
    std::fs::write(&file, b"s3cret")?;
    Ok(file.display().to_string())
}

/// Each required setting is named when it is absent.
#[test]
fn test_a_missing_setting_is_named() -> TestResult {
    let file = secret_file("missing")?;
    for (settings, named) in [
        (vec![format!("role_secret_file = \"{file}\"")], "cluster.id"),
        (vec!["id = 7".to_string()], "cluster.role_secret_file"),
    ] {
        let refused = RoleCluster::read(&start(&settings)?)
            .err()
            .context("a refusal")?;
        assert_eq!(refused.exit_code(), RoleError::EX_CONFIG);
        assert!(refused.to_string().contains(named), "{refused}");
    }
    Ok(())
}

/// What the table says reaches the cluster, the deadline defaulting to a minute.
#[test]
fn test_the_table_reaches_the_cluster() -> TestResult {
    let file = secret_file("read")?;
    let mut settings = vec![
        "id = 7".to_string(),
        format!("role_secret_file = \"{file}\""),
    ];
    let cluster = RoleCluster::read(&start(&settings)?)?;
    assert_eq!(cluster.id(), 7);
    assert_eq!(cluster.secret().password(), "s3cret");
    assert_eq!(cluster.deadline().as_secs(), 60);
    settings.push("join_deadline_ms = 250".to_string());
    assert_eq!(
        RoleCluster::read(&start(&settings)?)?
            .deadline()
            .as_millis(),
        250
    );
    Ok(())
}
