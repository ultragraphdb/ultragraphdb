/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fs;
use std::io;
use std::path::Path;

use crate::types::declaration_problem::DeclarationProblem;

/// Checks that every crate with a `features/` folder declares its feature suite for Cargo and Bazel, and that the
/// test script runs the `bdd-test` tag after the `unit-test` tag.
pub fn check_declarations(root: &Path) -> io::Result<Vec<DeclarationProblem>> {
    let mut problems = Vec::new();
    let mut crates: Vec<_> = fs::read_dir(root)?.collect::<Result<_, _>>()?;
    crates.sort_by_key(|e| e.file_name());
    for krate in crates
        .into_iter()
        .map(|e| e.path())
        .filter(|p| p.join("features").is_dir())
    {
        problems.extend(check_cargo(&krate.join("Cargo.toml"))?);
        problems.extend(check_bazel(&krate.join("BUILD.bazel"))?);
    }
    problems.extend(check_script(&root.join("scripts").join("test.sh"))?);
    Ok(problems)
}

fn check_cargo(manifest: &Path) -> io::Result<Vec<DeclarationProblem>> {
    let text = read_or_empty(manifest)?;
    let declared = text.split("[[test]]").skip(1).any(|block| {
        let block = block.split("\n[").next().unwrap_or("");
        let compact: String = block.split_whitespace().collect();
        compact.contains("name=\"bdd\"") && compact.contains("harness=false")
    });
    Ok(if declared {
        vec![]
    } else {
        vec![DeclarationProblem::new(
            manifest,
            "no [[test]] named \"bdd\" with harness = false",
        )]
    })
}

fn check_bazel(build: &Path) -> io::Result<Vec<DeclarationProblem>> {
    let text: String = read_or_empty(build)?.split_whitespace().collect();
    let mut problems = Vec::new();
    for (needle, what) in [
        (
            "use_libtest_harness=False",
            "a rust_test with use_libtest_harness = False",
        ),
        ("\"bdd-test\"", "the tag \"bdd-test\""),
        ("\"features/**/*.feature\"", "the feature files as data"),
    ] {
        if !text.contains(needle) {
            problems.push(DeclarationProblem::new(build, format!("missing {what}")));
        }
    }
    Ok(problems)
}

fn check_script(script: &Path) -> io::Result<Vec<DeclarationProblem>> {
    let text = read_or_empty(script)?;
    let unit = text.find("--test_tag_filters=unit-test");
    let bdd = text.find("--test_tag_filters=bdd-test");
    Ok(match (unit, bdd) {
        (Some(u), Some(b)) if b > u => vec![],
        _ => vec![DeclarationProblem::new(
            script,
            "no bazel test run filtered on bdd-test after the unit-test run",
        )],
    })
}

fn read_or_empty(path: &Path) -> io::Result<String> {
    match fs::read_to_string(path) {
        Ok(t) => Ok(t),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(e),
    }
}
