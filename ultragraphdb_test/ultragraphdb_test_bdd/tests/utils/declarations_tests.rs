/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fs;

use ultragraphdb_test_bdd::{check_declarations, workspace_root};

fn temp_root(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("ugdb-decl-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(d.join("scripts")).unwrap();
    d
}

#[test]
fn a_crate_with_features_and_no_suites_is_reported() {
    let root = temp_root("bare");
    fs::create_dir_all(root.join("ultragraphdb_x/features")).unwrap();
    fs::write(
        root.join("ultragraphdb_x/Cargo.toml"),
        "[package]\nname = \"x\"\n",
    )
    .unwrap();
    fs::write(root.join("scripts/test.sh"), "").unwrap();
    let problems = check_declarations(&root).unwrap();
    assert!(problems.iter().any(|p| p.problem().contains("[[test]]")));
    assert!(
        problems
            .iter()
            .any(|p| p.problem().contains("use_libtest_harness"))
    );
    assert!(problems.iter().any(|p| p.file().ends_with("test.sh")));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn a_crate_in_a_group_folder_is_checked() {
    let root = temp_root("group");
    fs::create_dir_all(root.join("ultragraphdb_g/ultragraphdb_x/features")).unwrap();
    fs::write(
        root.join("ultragraphdb_g/ultragraphdb_x/Cargo.toml"),
        "[package]\nname = \"x\"\n",
    )
    .unwrap();
    fs::write(root.join("scripts/test.sh"), "").unwrap();
    let problems = check_declarations(&root).unwrap();
    assert!(problems.iter().any(|p| {
        p.file()
            .ends_with("ultragraphdb_g/ultragraphdb_x/Cargo.toml")
    }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn the_workspace_declares_every_feature_suite() {
    let problems = check_declarations(&workspace_root()).unwrap();
    assert!(problems.is_empty(), "{problems:?}");
}
