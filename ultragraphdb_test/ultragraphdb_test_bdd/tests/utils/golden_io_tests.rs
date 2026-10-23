/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fs;

use ultragraphdb_test_bdd::{Golden, golden_files, load_golden, workspace_root};

#[test]
fn a_golden_file_loads_with_its_commit_and_cases() {
    let path = std::env::temp_dir().join(format!("ugdb-golden-{}.json", std::process::id()));
    fs::write(
        &path,
        r#"{"dgraph_commit":"9fbf35614","generator":"t","cases":[1,2,3]}"#,
    )
    .unwrap();
    let g: Golden<u32> = load_golden(&path);
    assert_eq!(g.dgraph_commit(), "9fbf35614");
    assert_eq!(g.generator(), "t");
    assert_eq!(g.cases(), &[1, 2, 3]);
    let _ = fs::remove_file(path);
}

#[test]
fn golden_files_are_found_in_group_folders() {
    let root = std::env::temp_dir().join(format!("ugdb-golden-dirs-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let dir = root.join("ultragraphdb_g/ultragraphdb_x/testdata/golden");
    fs::create_dir_all(&dir).unwrap();
    fs::write(root.join("ultragraphdb_g/ultragraphdb_x/Cargo.toml"), "").unwrap();
    fs::write(dir.join("a.json"), "{}").unwrap();
    assert_eq!(golden_files(&root).unwrap(), vec![dir.join("a.json")]);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn the_workspace_holds_golden_files() {
    let files = golden_files(&workspace_root()).unwrap();
    assert!(
        files
            .iter()
            .any(|f| f
                .ends_with("ultragraphdb_core/ultragraphdb_core_keys/testdata/golden/keys.json")),
        "{files:?}"
    );
}
