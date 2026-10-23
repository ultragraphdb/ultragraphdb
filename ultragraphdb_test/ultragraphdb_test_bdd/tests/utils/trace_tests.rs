/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_test_bdd::{
    FeatureScenario, SpecScenario, feature_scenarios, spec_scenarios, trace, workspace_root,
};

#[test]
fn names_are_traced_both_ways() {
    let specs = vec![
        SpecScenario::new("built", "a/spec.md", None),
        SpecScenario::new("unbuilt", "a/spec.md", None),
        SpecScenario::new("planned", "b/spec.md", Some("c".into())),
    ];
    let features = vec![
        FeatureScenario::new("built", "f"),
        FeatureScenario::new("stray", "f"),
    ];
    let r = trace(&specs, &features);
    assert_eq!(
        r.missing().iter().map(|s| s.name()).collect::<Vec<_>>(),
        vec!["unbuilt"]
    );
    assert_eq!(
        r.orphans().iter().map(|s| s.name()).collect::<Vec<_>>(),
        vec!["stray"]
    );
    assert_eq!(r.pending()["c"].len(), 1);
}

#[test]
fn the_workspace_traces() {
    let root = workspace_root();
    let report = trace(
        &spec_scenarios(&root).unwrap(),
        &feature_scenarios(&root).unwrap(),
    );
    println!("{report}");
    assert!(report.passed(), "{report}");
}
