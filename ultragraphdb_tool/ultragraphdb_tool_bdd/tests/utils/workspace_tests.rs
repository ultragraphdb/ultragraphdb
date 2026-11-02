/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::env;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::process::{Command, Output};

use ultragraphdb_tool_bdd::{Context, TestError, TestResult, features_dir, testdata_dir};

/// Set in a child run of this binary to make `child_reports_the_dirs` print what it finds. Where it finds none, the
/// child fails, and libtest prints the returned error to stderr as `Error: <its Debug>`.
const CHILD: &str = "UGDB_BDD_WORKSPACE_CHILD";

#[test]
fn child_reports_the_dirs() -> TestResult {
    if env::var_os(CHILD).is_some() {
        println!("features_dir={}", features_dir()?.display());
        println!("testdata_dir={}", testdata_dir()?.display());
    }
    Ok(())
}

/// Runs `child_reports_the_dirs` in a fresh process with only the given build-system variables set.
fn run_child(set: &[(&str, &OsStr)]) -> TestResult<Output> {
    let mut cmd = Command::new(env::current_exe()?);
    let child = cmd
        .args([
            "utils::workspace_tests::child_reports_the_dirs",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, "1")
        .env_remove("TEST_SRCDIR")
        .env_remove("TEST_WORKSPACE")
        .env_remove("TEST_TARGET")
        .env_remove("CARGO_MANIFEST_DIR")
        .envs(set.iter().copied());
    Ok(child.output()?)
}

/// Runs `child_reports_the_dirs` in a fresh process with the given environment, and returns the two directories it
/// printed, or `None` if the child failed. libtest prints the first after its own `test <name> ...` on one line.
fn dirs_in(set: &[(&str, &str)]) -> TestResult<Option<(String, String)>> {
    let os_set: Vec<(&str, &OsStr)> = set.iter().map(|(k, v)| (*k, OsStr::new(*v))).collect();
    let out = run_child(&os_set)?;
    if !out.status.success() {
        return Ok(None);
    }
    let stdout = String::from_utf8(out.stdout)?;
    let find = |label: &str| {
        stdout
            .lines()
            .find_map(|l| l.split_once(label).map(|(_, dir)| dir))
            .map(str::to_string)
            .context(&format!("the child prints {label} in {stdout}"))
    };
    Ok(Some((find("features_dir=")?, find("testdata_dir=")?)))
}

#[test]
fn under_bazel_the_package_is_read_from_the_test_target() -> TestResult {
    // A label names its package between the repository's `//` and the target's `:`, whatever `@` prefix it carries.
    let targets = [
        "//a/b:c_tests",
        "@//a/b:c_tests",
        "@@//a/b:c_tests",
        "//a/b:bdd",
    ];
    for target in targets {
        let dirs = dirs_in(&[
            ("TEST_SRCDIR", "/runfiles"),
            ("TEST_WORKSPACE", "_main"),
            ("TEST_TARGET", target),
            // Bazel's branch wins even where Cargo's variable is also set.
            ("CARGO_MANIFEST_DIR", "/manifest"),
        ])?;
        assert_eq!(
            dirs,
            Some((
                "/runfiles/_main/a/b/features".to_string(),
                "/runfiles/_main/a/b/testdata".to_string()
            )),
            "{target}"
        );
    }
    Ok(())
}

#[test]
fn a_root_package_target_resolves_to_the_workspace() -> TestResult {
    let dirs = dirs_in(&[
        ("TEST_SRCDIR", "/runfiles"),
        ("TEST_WORKSPACE", "_main"),
        ("TEST_TARGET", "//:root_tests"),
    ])?;
    assert_eq!(
        dirs,
        Some((
            "/runfiles/_main/features".to_string(),
            "/runfiles/_main/testdata".to_string()
        ))
    );
    Ok(())
}

#[test]
fn without_every_bazel_variable_the_cargo_manifest_dir_is_used() -> TestResult {
    let partial: [&[(&str, &str)]; 3] = [
        &[("TEST_WORKSPACE", "_main"), ("TEST_TARGET", "//a/b:c")],
        &[("TEST_SRCDIR", "/runfiles"), ("TEST_TARGET", "//a/b:c")],
        &[("TEST_SRCDIR", "/runfiles"), ("TEST_WORKSPACE", "_main")],
    ];
    for set in partial {
        let mut env: Vec<(&str, &str)> = set.to_vec();
        env.push(("CARGO_MANIFEST_DIR", "/manifest"));
        assert_eq!(
            dirs_in(&env)?,
            Some((
                "/manifest/features".to_string(),
                "/manifest/testdata".to_string()
            )),
            "{set:?}"
        );
    }
    Ok(())
}

#[test]
fn without_bazel_or_cargo_variables_the_dirs_fail_naming_the_variable() -> TestResult {
    let cases: [(&[(&str, &OsStr)], &str); 4] = [
        (
            &[],
            "CARGO_MANIFEST_DIR is set by cargo for tests: environment variable not found",
        ),
        (
            &[("CARGO_MANIFEST_DIR", OsStr::from_bytes(b"/m\xff"))],
            r#"CARGO_MANIFEST_DIR is set by cargo for tests: environment variable was not valid unicode: "/m\xFF""#,
        ),
        // Two of Bazel's three variables leave the choice to Cargo's, which is absent.
        (
            &[
                ("TEST_SRCDIR", OsStr::new("/runfiles")),
                ("TEST_WORKSPACE", OsStr::new("_main")),
            ],
            "CARGO_MANIFEST_DIR is set by cargo for tests: environment variable not found",
        ),
        // All three of Bazel's, one of them not Unicode, count as unset.
        (
            &[
                ("TEST_SRCDIR", OsStr::new("/runfiles")),
                ("TEST_WORKSPACE", OsStr::new("_main")),
                ("TEST_TARGET", OsStr::from_bytes(b"//a/\xff:c")),
            ],
            "CARGO_MANIFEST_DIR is set by cargo for tests: environment variable not found",
        ),
    ];
    for (set, message) in cases {
        let out = run_child(set)?;
        assert!(!out.status.success(), "{set:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        let reported = format!("Error: {:?}", TestError::new(message));
        assert!(stderr.lines().any(|l| l == reported), "{set:?}: {stderr}");
    }
    Ok(())
}
