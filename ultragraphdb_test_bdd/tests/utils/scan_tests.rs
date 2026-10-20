/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fs;

use ultragraphdb_test_bdd::{feature_scenarios, spec_scenarios};

#[test]
fn archived_change_and_feature_scenarios_are_found() {
    let root = std::env::temp_dir().join(format!("ugdb-scan-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    for (rel, text) in [
        ("openspec/specs/a/spec.md", "#### Scenario: A one\n"),
        (
            "openspec/changes/c1/specs/b/spec.md",
            "#### Scenario: B one\n#### Scenario: B two\n",
        ),
        (
            "openspec/changes/archive/old/specs/z/spec.md",
            "#### Scenario: Archived change\n",
        ),
        ("k/features/x.feature", "Feature: F\n  Scenario: A one\n"),
        (
            "k/target/features/y.feature",
            "Feature: F\n  Scenario: Build output\n",
        ),
    ] {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    let specs = spec_scenarios(&root).unwrap();
    let names: Vec<_> = specs.iter().map(|s| (s.name(), s.change())).collect();
    assert_eq!(
        names,
        vec![
            ("A one", None),
            ("B one", Some("c1")),
            ("B two", Some("c1"))
        ]
    );
    let features = feature_scenarios(&root).unwrap();
    assert_eq!(
        features.iter().map(|f| f.name()).collect::<Vec<_>>(),
        vec!["A one"]
    );
    let _ = fs::remove_dir_all(root);
}
