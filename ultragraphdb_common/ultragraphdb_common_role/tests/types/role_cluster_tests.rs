/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_role::RoleCluster;
use ultragraphdb_common_service::RoleStart;
use ultragraphdb_error_role::RoleError;
use ultragraphdb_tool_bdd::{Context, TestResult};

fn start(settings: &[(&str, String)]) -> TestResult<RoleStart> {
    let mut variables = vec![(
        "ULTRAGRAPHDB_OBSERVABILITY_ADDRESS".to_string(),
        "tcp://127.0.0.1:1".to_string(),
    )];
    variables.extend(
        settings
            .iter()
            .map(|(name, value)| (name.to_string(), value.clone())),
    );
    Ok(RoleStart::read(&[], variables)?)
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
        (
            vec![("ULTRAGRAPHDB_CLUSTER_ROLE_SECRET_FILE", file)],
            "cluster.id",
        ),
        (
            vec![("ULTRAGRAPHDB_CLUSTER_ID", "7".to_string())],
            "cluster.role_secret_file",
        ),
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
        ("ULTRAGRAPHDB_CLUSTER_ID", "7".to_string()),
        ("ULTRAGRAPHDB_CLUSTER_ROLE_SECRET_FILE", file),
    ];
    let cluster = RoleCluster::read(&start(&settings)?)?;
    assert_eq!(cluster.id(), 7);
    assert_eq!(cluster.secret().password(), "s3cret");
    assert_eq!(cluster.deadline().as_secs(), 60);
    settings.push(("ULTRAGRAPHDB_CLUSTER_JOIN_DEADLINE_MS", "250".to_string()));
    assert_eq!(
        RoleCluster::read(&start(&settings)?)?
            .deadline()
            .as_millis(),
        250
    );
    Ok(())
}
