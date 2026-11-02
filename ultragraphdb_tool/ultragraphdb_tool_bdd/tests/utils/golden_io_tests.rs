/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use ultragraphdb_tool_bdd::{Golden, TestError, TestResult, load_golden};

/// A file in the temp directory, named for the test that wrote it and removed when the test ends, even on a failure.
struct TempFile(PathBuf);

impl TempFile {
    fn new(name: &str, contents: &str) -> TestResult<Self> {
        let path =
            std::env::temp_dir().join(format!("ugdb-golden-{}-{name}.json", std::process::id()));
        fs::write(&path, contents)?;
        Ok(TempFile(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        // Removal is best effort: a drop must not panic, and a leftover file is harmless.
        let _removed: std::io::Result<()> = fs::remove_file(&self.0);
    }
}

/// The message `load_golden` failed with, loading `u32` cases.
fn load_failure(path: &Path) -> TestResult<String> {
    let Err(e) = load_golden::<u32>(path) else {
        return Err(TestError::new("load_golden must fail"));
    };
    Ok(e.reason().to_string())
}

#[derive(Debug, PartialEq, Deserialize)]
struct Case {
    input: String,
    bytes: Vec<u8>,
}

#[test]
fn a_golden_file_loads_with_its_commit_and_cases() -> TestResult {
    let f = TempFile::new(
        "scalars",
        r#"{"dgraph_commit":"9fbf35614","generator":"t","cases":[1,2,3]}"#,
    )?;
    let g: Golden<u32> = load_golden(f.path())?;
    assert_eq!(g.dgraph_commit(), "9fbf35614");
    assert_eq!(g.generator(), "t");
    assert_eq!(g.cases(), &[1, 2, 3]);
    Ok(())
}

#[test]
fn a_golden_file_keeps_its_cases_in_order_with_their_fields() -> TestResult {
    let f = TempFile::new(
        "structs",
        r#"{"generator":"tools/golden x","cases":[{"input":"b","bytes":[255,0]},{"input":"","bytes":[]},{"input":"a","bytes":[1]}],"dgraph_commit":"c"}"#,
    )?;
    let g: Golden<Case> = load_golden(f.path())?;
    assert_eq!(g.dgraph_commit(), "c");
    assert_eq!(g.generator(), "tools/golden x");
    assert_eq!(
        g.cases(),
        &[
            Case {
                input: "b".into(),
                bytes: vec![255, 0]
            },
            Case {
                input: String::new(),
                bytes: vec![]
            },
            Case {
                input: "a".into(),
                bytes: vec![1]
            },
        ]
    );
    Ok(())
}

#[test]
fn a_golden_file_without_cases_loads_empty() -> TestResult {
    let f = TempFile::new("empty", r#"{"dgraph_commit":"","generator":"","cases":[]}"#)?;
    let g: Golden<u32> = load_golden(f.path())?;
    assert_eq!(g.dgraph_commit(), "");
    assert_eq!(g.generator(), "");
    assert!(g.cases().is_empty());
    Ok(())
}

#[test]
fn a_missing_golden_file_fails_naming_the_path() -> TestResult {
    let path = std::env::temp_dir().join(format!("ugdb-golden-{}-absent.json", std::process::id()));
    let msg = load_failure(&path)?;
    assert_eq!(
        msg,
        format!(
            "cannot read golden file {}: No such file or directory (os error 2)",
            path.display()
        )
    );
    Ok(())
}

#[test]
fn a_malformed_golden_file_fails_naming_the_path_and_the_fault() -> TestResult {
    let cases: &[(&str, &str, &str)] = &[
        ("truncated", "{", "EOF while parsing an object"),
        ("not-json", "dgraph", "expected value"),
        (
            "no-generator",
            r#"{"dgraph_commit":"c","cases":[]}"#,
            "missing field `generator`",
        ),
        (
            "no-commit",
            r#"{"generator":"g","cases":[]}"#,
            "missing field `dgraph_commit`",
        ),
        (
            "no-cases",
            r#"{"dgraph_commit":"c","generator":"g"}"#,
            "missing field `cases`",
        ),
        (
            "wrong-case-type",
            r#"{"dgraph_commit":"c","generator":"g","cases":["a"]}"#,
            "invalid type: string \"a\", expected u32",
        ),
        (
            "case-out-of-range",
            r#"{"dgraph_commit":"c","generator":"g","cases":[-1]}"#,
            "invalid value: integer `-1`, expected u32",
        ),
    ];
    for (name, contents, fault) in cases {
        let f = TempFile::new(name, contents)?;
        let msg = load_failure(f.path())?;
        let prefix = format!("cannot parse golden file {}: ", f.path().display());
        assert!(msg.starts_with(&prefix), "{name}: {msg}");
        assert!(msg[prefix.len()..].starts_with(fault), "{name}: {msg}");
    }
    Ok(())
}
