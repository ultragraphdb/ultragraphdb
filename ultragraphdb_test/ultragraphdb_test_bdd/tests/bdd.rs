/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use cucumber::{World, given, then, when};
use ultragraphdb_test_bdd::{
    DeclarationProblem, TraceReport, check_declarations, compare_bytes, feature_scenarios,
    features_dir, golden_files, spec_scenarios, trace, workspace_root,
};

#[derive(Debug, Default, World)]
struct HarnessWorld {
    dir: Option<PathBuf>,
    report: Option<TraceReport>,
    problems: Vec<DeclarationProblem>,
    script_problems: Vec<DeclarationProblem>,
    golden: Vec<(PathBuf, serde_json::Value)>,
    expected: Vec<u8>,
    input: String,
    compared: Option<Result<(), String>>,
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

impl HarnessWorld {
    fn dir(&mut self) -> PathBuf {
        self.dir
            .get_or_insert_with(|| {
                let d = std::env::temp_dir().join(format!(
                    "ugdb-bdd-{}-{}",
                    std::process::id(),
                    COUNTER.fetch_add(1, Ordering::SeqCst)
                ));
                fs::create_dir_all(&d).unwrap();
                d
            })
            .clone()
    }

    fn write(&mut self, rel: &str, text: &str) {
        let path = self.dir().join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}

fn spec(name: &str) -> String {
    format!(
        "## ADDED Requirements\n\n### Requirement: R\nThe system SHALL hold.\n\n#### Scenario: {name}\n- **WHEN** x\n- **THEN** y\n"
    )
}

#[given(expr = "an archived spec declaring the scenario {string}")]
fn archived_spec(w: &mut HarnessWorld, name: String) {
    w.write("openspec/specs/cap/spec.md", &spec(&name));
}

#[given(expr = "the change {string} declaring the scenario {string}")]
fn change_spec(w: &mut HarnessWorld, change: String, name: String) {
    w.write(
        &format!("openspec/changes/{change}/specs/cap/spec.md"),
        &spec(&name),
    );
}

#[given(expr = "two specs declaring the scenario {string}")]
fn two_specs(w: &mut HarnessWorld, name: String) {
    w.write("openspec/specs/one/spec.md", &spec(&name));
    w.write("openspec/specs/two/spec.md", &spec(&name));
    w.write(
        "crate_a/features/f.feature",
        &format!("Feature: F\n  Scenario: {name}\n    Given x\n"),
    );
}

#[given(expr = "a feature naming the scenario {string}")]
fn feature(w: &mut HarnessWorld, name: String) {
    w.write(
        "crate_a/features/f.feature",
        &format!("Feature: F\n  Scenario: {name}\n    Given x\n"),
    );
}

#[given("no feature naming it")]
#[given("no spec declaring it")]
fn nothing(_w: &mut HarnessWorld) {}

#[when("the traceability test runs")]
fn run_trace(w: &mut HarnessWorld) {
    let root = w.dir();
    w.report = Some(trace(
        &spec_scenarios(&root).unwrap(),
        &feature_scenarios(&root).unwrap(),
    ));
}

#[then(expr = "it fails naming {string} and its spec file")]
fn fails_missing(w: &mut HarnessWorld, name: String) {
    let r = w.report.as_ref().unwrap();
    assert!(!r.passed());
    let text = r.to_string();
    assert!(
        text.contains(&name) && text.contains("openspec/specs/cap/spec.md"),
        "{text}"
    );
}

#[then(expr = "it fails naming {string} and its feature file")]
fn fails_orphan(w: &mut HarnessWorld, name: String) {
    let r = w.report.as_ref().unwrap();
    assert!(!r.passed());
    let text = r.to_string();
    assert!(
        text.contains(&name) && text.contains("crate_a/features/f.feature"),
        "{text}"
    );
}

#[then(expr = "it passes and lists {string} as pending in {string}")]
fn passes_pending(w: &mut HarnessWorld, name: String, change: String) {
    let r = w.report.as_ref().unwrap();
    assert!(r.passed(), "{r}");
    assert!(r.pending()[&change].iter().any(|s| s.name() == name));
}

#[then(expr = "it fails naming {string} and both spec files")]
fn fails_duplicate(w: &mut HarnessWorld, name: String) {
    let r = w.report.as_ref().unwrap();
    assert!(!r.passed());
    let text = r.to_string();
    assert!(
        text.contains(&name) && text.contains("one/spec.md") && text.contains("two/spec.md"),
        "{text}"
    );
}

#[when("the declaration test reads each crate holding a features folder")]
fn read_crates(w: &mut HarnessWorld) {
    w.problems = check_declarations(&workspace_root()).unwrap();
}

#[then("each declares the Cargo test and the Bazel target as required")]
fn crates_declared(w: &mut HarnessWorld) {
    let crate_problems: Vec<_> = w
        .problems
        .iter()
        .filter(|p| !p.file().ends_with("test.sh"))
        .collect();
    assert!(crate_problems.is_empty(), "{crate_problems:?}");
}

#[when("the declaration test reads the test script")]
fn read_script(w: &mut HarnessWorld) {
    w.script_problems = check_declarations(&workspace_root())
        .unwrap()
        .into_iter()
        .filter(|p| p.file().ends_with("test.sh"))
        .collect();
}

#[then("it finds a Bazel test run filtered on bdd-test after the run filtered on unit-test")]
fn script_declared(w: &mut HarnessWorld) {
    assert!(w.script_problems.is_empty(), "{:?}", w.script_problems);
}

#[when("the golden files of the workspace are read")]
fn read_golden(w: &mut HarnessWorld) {
    w.golden = golden_files(&workspace_root())
        .unwrap()
        .into_iter()
        .map(|p| {
            let v = serde_json::from_str(&fs::read_to_string(&p).unwrap()).unwrap();
            (p, v)
        })
        .collect();
}

#[then("each records the Dgraph commit it was generated from")]
fn golden_commits(w: &mut HarnessWorld) {
    for (path, v) in &w.golden {
        let commit = v
            .get("dgraph_commit")
            .and_then(|c| c.as_str())
            .unwrap_or("");
        assert!(
            commit.len() >= 7,
            "{} records no Dgraph commit",
            path.display()
        );
    }
}

fn parse_hex(s: &str) -> Vec<u8> {
    s.split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).unwrap())
        .collect()
}

#[given(expr = "the golden bytes {string} for the input {string}")]
fn golden_bytes(w: &mut HarnessWorld, bytes: String, input: String) {
    w.expected = parse_hex(&bytes);
    w.input = input;
}

#[when(expr = "the output {string} is compared with them")]
fn compare(w: &mut HarnessWorld, bytes: String) {
    w.compared = Some(compare_bytes(&w.input, &w.expected, &parse_hex(&bytes)));
}

#[then(expr = "the comparison fails and reports {string}, {string} and {string}")]
fn compare_fails(w: &mut HarnessWorld, input: String, expected: String, actual: String) {
    let err = w.compared.clone().unwrap().unwrap_err();
    assert!(
        err.contains(&input) && err.contains(&expected) && err.contains(&actual),
        "{err}"
    );
}

fn cleanup(dir: &Path) {
    let _ = fs::remove_dir_all(dir);
}

fn main() {
    let features = features_dir();
    futures::executor::block_on(
        HarnessWorld::cucumber()
            .fail_on_skipped()
            .after(|_, _, _, _, world| {
                Box::pin(async move {
                    if let Some(dir) = world.and_then(|w| w.dir.clone()) {
                        cleanup(&dir);
                    }
                })
            })
            .run_and_exit(features),
    );
}
