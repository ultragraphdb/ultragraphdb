/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_service::{Bootstrap, RoleKind};
use ultragraphdb_tool_bdd::{Context, TestResult};

const URL: &str = "ULTRAGRAPHDB_CONFIG_URL";
const TARGET: &str = "ULTRAGRAPHDB_CONFIG_TARGET";
const INSTANCE: &str = "ULTRAGRAPHDB_CONFIG_INSTANCE";
const KEY_FILE: &str = "ULTRAGRAPHDB_CONFIG_KEY_FILE";
const CA_FILE: &str = "ULTRAGRAPHDB_CONFIG_CA_FILE";
const PARENT: &str = "ULTRAGRAPHDB_PARENT_PID";

/// A file holding a read key, named for `test`, so tests beside it hold their own.
fn key_file(test: &str) -> TestResult<String> {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, Into::into);
    let file = std::path::Path::new(&root).join(format!("bootstrap-{test}-{}", std::process::id()));
    std::fs::write(&file, "a read key of at least thirty two bytes")?;
    Ok(file.display().to_string())
}

fn variables(key: &str) -> Vec<(String, String)> {
    [
        (URL, "http://127.0.0.1:1"),
        (TARGET, "ci"),
        (INSTANCE, "storage_1"),
        (KEY_FILE, key),
    ]
    .map(|(name, value)| (name.to_string(), value.to_string()))
    .to_vec()
}

/// `set` with `name` holding `value`, in place of what it held.
fn with(mut set: Vec<(String, String)>, name: &str, value: &str) -> Vec<(String, String)> {
    set.retain(|(held, _)| held != name);
    set.push((name.to_string(), value.to_string()));
    set
}

fn without(mut set: Vec<(String, String)>, name: &str) -> Vec<(String, String)> {
    set.retain(|(held, _)| held != name);
    set
}

#[test]
fn test_the_five_variables_name_the_service_and_the_instance() -> TestResult {
    let key = key_file("reads")?;
    let read = Bootstrap::read(RoleKind::Storage, variables(&key))?;
    assert_eq!(read.target(), "ci");
    assert_eq!(read.instance(), "storage_1");
    assert_eq!(read.revision(), None);
    assert_eq!(read.parent(), None);
    let pid = std::process::id();
    let harnessed = with(variables(&key), PARENT, &pid.to_string());
    assert_eq!(
        Bootstrap::read(RoleKind::Storage, harnessed)?.parent(),
        Some(pid)
    );
    let loading = with(variables(&key), "ULTRAGRAPHDB_CONFIG_REVISION", "8");
    assert_eq!(
        Bootstrap::read(RoleKind::Storage, loading)?.revision(),
        Some(8)
    );
    let others = with(variables(&key), "PATH", "/bin");
    assert!(
        Bootstrap::read(RoleKind::Storage, others).is_ok(),
        "another program's variable is left alone"
    );
    let role_named = with(variables(&key), INSTANCE, "storage");
    assert_eq!(
        Bootstrap::read(RoleKind::Storage, role_named)?.instance(),
        "storage"
    );
    Ok(())
}

/// Each refusal is a configuration error naming what it refused.
#[test]
fn test_what_a_role_cannot_use_is_refused_by_name() -> TestResult {
    let key = key_file("refuses")?;
    let all = || variables(&key);
    let past_any_process = (u64::from(u32::MAX) + 1).to_string();
    let cases = [
        (without(all(), URL), URL),
        (without(all(), TARGET), TARGET),
        (without(all(), INSTANCE), INSTANCE),
        (without(all(), KEY_FILE), KEY_FILE),
        (
            with(all(), "ULTRAGRAPHDB_STORAGE_PARTITION_COUNT", "2"),
            "ULTRAGRAPHDB_STORAGE_PARTITION_COUNT",
        ),
        (with(all(), INSTANCE, "compute_1"), "compute_1"),
        (with(all(), INSTANCE, "storage_"), "storage_"),
        (with(all(), INSTANCE, "storages"), "storages"),
        (with(all(), URL, "ftp://x"), URL),
        (with(all(), URL, "https://127.0.0.1:1"), CA_FILE),
        (
            with(
                with(all(), URL, "https://127.0.0.1:1"),
                CA_FILE,
                "/nonexistent/ca",
            ),
            CA_FILE,
        ),
        (with(all(), KEY_FILE, "/nonexistent/key"), KEY_FILE),
        (
            with(all(), "ULTRAGRAPHDB_CONFIG_REVISION", "latest"),
            "ULTRAGRAPHDB_CONFIG_REVISION",
        ),
        (with(all(), PARENT, "the test"), PARENT),
        (with(all(), PARENT, &past_any_process), PARENT),
    ];
    for (set, named) in cases {
        let refused = Bootstrap::read(RoleKind::Storage, set)
            .err()
            .context("a refusal")?;
        assert_eq!(refused.exit_code(), 78, "{refused}");
        assert!(
            refused.to_string().contains(named),
            "{refused} names {named}"
        );
    }
    let compute = Bootstrap::read(RoleKind::Compute, all())
        .err()
        .context("a refusal")?;
    assert!(compute.to_string().contains("compute"), "{compute}");
    Ok(())
}
