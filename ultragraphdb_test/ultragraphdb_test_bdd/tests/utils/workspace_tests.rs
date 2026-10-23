/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_test_bdd::{features_dir, testdata_dir, workspace_root};

#[test]
fn the_workspace_root_holds_the_module_file() {
    assert!(workspace_root().join("MODULE.bazel").is_file());
}

#[test]
fn this_crate_finds_its_features() {
    assert!(features_dir().join("port-testing").is_dir());
}

#[test]
fn testdata_sits_beside_the_features() {
    assert_eq!(testdata_dir().parent(), features_dir().parent());
}
