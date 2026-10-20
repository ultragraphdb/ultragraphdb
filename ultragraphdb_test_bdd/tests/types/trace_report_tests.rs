/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use ultragraphdb_test_bdd::{SpecScenario, TraceReport};

#[test]
fn pending_scenarios_do_not_fail_the_trace() {
    let mut pending = BTreeMap::new();
    pending.insert(
        "c".to_string(),
        vec![SpecScenario::new("s", "x/spec.md", Some("c".into()))],
    );
    let r = TraceReport::new(vec![], vec![], vec![], pending);
    assert!(r.passed());
    assert!(r.to_string().contains("pending in c: 1 scenarios"));
}

#[test]
fn a_missing_scenario_fails_the_trace() {
    let r = TraceReport::new(
        vec![SpecScenario::new("s", "x/spec.md", None)],
        vec![],
        vec![],
        BTreeMap::new(),
    );
    assert!(!r.passed());
    assert!(r.to_string().contains("missing feature: \"s\""));
}
