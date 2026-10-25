/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::env;
use std::path::Path;
use std::process::Command;

use ultragraphdb_test_bdd::{features_dir, testdata_dir};

/// The crate's own path in the workspace, which both build systems place the package at.
const PACKAGE: &str = "ultragraphdb_test/ultragraphdb_test_bdd";

/// Set in a child run of this binary to make `child_reports_the_dirs` print what it finds.
const CHILD: &str = "UGDB_BDD_WORKSPACE_CHILD";

#[test]
fn this_crate_finds_its_features() {
    let dir = features_dir();
    assert!(
        dir.ends_with(Path::new(PACKAGE).join("features")),
        "{}",
        dir.display()
    );
    assert!(
        dir.join("port-testing/port_testing.feature").is_file(),
        "{}",
        dir.display()
    );
}

#[test]
fn testdata_sits_beside_the_features() {
    let dir = testdata_dir();
    assert!(
        dir.ends_with(Path::new(PACKAGE).join("testdata")),
        "{}",
        dir.display()
    );
    assert_eq!(dir, features_dir().with_file_name("testdata"));
}

/// Prints both directories when run as a child; passes silently in an ordinary run.
#[test]
fn child_reports_the_dirs() {
    if env::var_os(CHILD).is_some() {
        println!("features_dir={}", features_dir().display());
        println!("testdata_dir={}", testdata_dir().display());
    }
}

/// Runs `child_reports_the_dirs` in a fresh process with the given environment, and returns the two directories it
/// printed, or `None` if the child failed. libtest prints the first after its own `test <name> ...` on one line.
fn dirs_in(set: &[(&str, &str)]) -> Option<(String, String)> {
    let mut cmd = Command::new(env::current_exe().unwrap());
    cmd.args([
        "utils::workspace_tests::child_reports_the_dirs",
        "--exact",
        "--nocapture",
        "--test-threads=1",
    ])
    .env(CHILD, "1");
    for key in [
        "TEST_SRCDIR",
        "TEST_WORKSPACE",
        "TEST_TARGET",
        "CARGO_MANIFEST_DIR",
    ] {
        cmd.env_remove(key);
    }
    for (key, value) in set {
        cmd.env(key, value);
    }
    let out = cmd.output().unwrap();
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8(out.stdout).unwrap();
    let find = |label: &str| {
        stdout
            .lines()
            .find_map(|l| l.split_once(label).map(|(_, dir)| dir))
            .map(str::to_string)
            .unwrap_or_else(|| panic!("no {label} in {stdout}"))
    };
    Some((find("features_dir="), find("testdata_dir=")))
}

#[test]
fn under_bazel_the_package_is_read_from_the_test_target() {
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
        ]);
        assert_eq!(
            dirs,
            Some((
                "/runfiles/_main/a/b/features".to_string(),
                "/runfiles/_main/a/b/testdata".to_string()
            )),
            "{target}"
        );
    }
}

#[test]
fn a_root_package_target_resolves_to_the_workspace() {
    let dirs = dirs_in(&[
        ("TEST_SRCDIR", "/runfiles"),
        ("TEST_WORKSPACE", "_main"),
        ("TEST_TARGET", "//:root_tests"),
    ]);
    assert_eq!(
        dirs,
        Some((
            "/runfiles/_main/features".to_string(),
            "/runfiles/_main/testdata".to_string()
        ))
    );
}

#[test]
fn without_every_bazel_variable_the_cargo_manifest_dir_is_used() {
    let partial: [&[(&str, &str)]; 3] = [
        &[("TEST_WORKSPACE", "_main"), ("TEST_TARGET", "//a/b:c")],
        &[("TEST_SRCDIR", "/runfiles"), ("TEST_TARGET", "//a/b:c")],
        &[("TEST_SRCDIR", "/runfiles"), ("TEST_WORKSPACE", "_main")],
    ];
    for set in partial {
        let mut env: Vec<(&str, &str)> = set.to_vec();
        env.push(("CARGO_MANIFEST_DIR", "/manifest"));
        assert_eq!(
            dirs_in(&env),
            Some((
                "/manifest/features".to_string(),
                "/manifest/testdata".to_string()
            )),
            "{set:?}"
        );
    }
}
