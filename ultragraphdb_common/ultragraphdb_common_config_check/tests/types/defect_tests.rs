/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_config_check::Defect;

/// An entry reads complete on its own, leaving out only what it does not know.
#[test]
fn test_an_entry_reads_complete() {
    let full = Defect::new("it holds 0", "it must be at least 1")
        .in_file("ci.toml")
        .on_line(Some(3))
        .of_target("ci")
        .of_instance("storage_1")
        .of_setting("storage.partition_count");
    assert_eq!(
        full.to_string(),
        "ci.toml:3: target ci, instance storage_1, setting storage.partition_count: it holds 0; it must be at least 1"
    );
    assert_eq!(full.file(), Some("ci.toml"));
    assert_eq!(full.line(), Some(3));
    assert_eq!(full.setting(), Some("storage.partition_count"));
    let bare = Defect::new("found", "required");
    assert_eq!(bare.to_string(), "found; required");
    assert_eq!(
        Defect::new("f", "r").in_file("a.toml").to_string(),
        "a.toml: f; r"
    );
}

/// A defect standing in a stored revision names it after its target.
#[test]
fn test_an_entry_names_its_revision() {
    let revision = 4;
    let stored = Defect::new("found", "required")
        .of_target("ci")
        .of_revision(revision);
    assert_eq!(
        stored.to_string(),
        format!("target ci, revision {revision}, found; required")
    );
}
