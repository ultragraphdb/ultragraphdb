/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_service::RoleStart;
use ultragraphdb_tool_bdd::{Context, TestResult};

const ADDRESS: (&str, &str) = (
    "ULTRAGRAPHDB_OBSERVABILITY_ADDRESS",
    "tcp://127.0.0.1:18080",
);

fn set(name: &str, value: &str) -> (String, String) {
    (name.to_string(), value.to_string())
}

#[test]
fn test_a_start_reads_its_address_from_the_environment() -> TestResult {
    let start = RoleStart::read(&[], [set(ADDRESS.0, ADDRESS.1)])?;
    let expected = ADDRESS.1.strip_prefix("tcp://").context("a tcp address")?;
    assert_eq!(start.observability().to_string(), expected);
    assert_eq!(start.config().observability().address(), Some(ADDRESS.1));
    Ok(())
}

#[test]
fn test_a_start_takes_a_named_file() -> TestResult {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, Into::into);
    let file = std::path::Path::new(&root).join(format!("role-start-{}.toml", std::process::id()));
    std::fs::write(
        &file,
        "[observability]\naddress = \"tcp://127.0.0.1:18081\"\n",
    )?;
    let args = ["--config".to_string(), file.display().to_string()];
    let start = RoleStart::read(&args, [])?;
    assert_eq!(start.observability().port(), 18081);
    std::fs::remove_file(&file)?;
    Ok(())
}

/// Each is a configuration error, naming what it refused.
#[test]
fn test_a_start_refuses_what_it_cannot_use() -> TestResult {
    let cases = [
        (
            vec!["--other".to_string(), "x".to_string()],
            vec![set(ADDRESS.0, ADDRESS.1)],
            "--other",
        ),
        (
            vec!["--other".to_string()],
            vec![set(ADDRESS.0, ADDRESS.1)],
            "--other",
        ),
        (
            vec!["--config".to_string()],
            vec![set(ADDRESS.0, ADDRESS.1)],
            "--config",
        ),
        (vec![], vec![], "observability.address"),
        (
            vec![],
            vec![set(ADDRESS.0, "127.0.0.1:18080")],
            "observability.address",
        ),
        (
            vec![],
            vec![set(ADDRESS.0, "tcp://nowhere")],
            "observability.address",
        ),
        (
            vec!["--config".to_string(), "/nonexistent/x.toml".to_string()],
            vec![],
            "/nonexistent/x.toml",
        ),
    ];
    for (args, variables, named) in cases {
        let refused = RoleStart::read(&args, variables)
            .err()
            .context("a refusal")?;
        assert_eq!(refused.exit_code(), 78, "{refused}");
        assert!(
            refused.to_string().contains(named),
            "{refused} names {named}"
        );
    }
    Ok(())
}
