/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;
use std::path::PathBuf;

use serde_json::Value;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_bench::{AllocArgs, AllocCount, BenchErrorEnum, run_allocs};

/// Counts moved by hand: each read adds one allocation, two reallocations and 100 bytes, so each step between two reads
/// counts exactly those.
#[derive(Default)]
struct Stepped(Cell<u64>);

impl AllocCount for Stepped {
    fn counted(&self) -> (u64, u64, u64) {
        let n = self.0.get() + 1;
        self.0.set(n);
        (n, 2 * n, 100 * n)
    }
}

fn fresh(name: &str) -> TestResult<PathBuf> {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let dir = root.join(format!("bench-allocs-{name}-{}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(dir)
}

fn parsed(dir: &std::path::Path, relations: &str) -> TestResult<AllocArgs> {
    let owned: Vec<String> = [
        "--frames",
        "20",
        "--relations",
        relations,
        "--hubs",
        "3",
        "--dir",
    ]
    .iter()
    .map(ToString::to_string)
    .chain([dir.display().to_string()])
    .collect();
    Ok(AllocArgs::parse(&owned)?)
}

fn figure(report: &Value, path: &[&str]) -> TestResult<f64> {
    path.iter()
        .try_fold(report, |value, field| value.get(field))
        .and_then(Value::as_f64)
        .context(&path.join("."))
}

/// Every frame commits, and the report splits each frame's count into its three steps: one each here, three in all, over
/// a frame's terms, the subject, its hub, the two predicates and the string.
#[test]
fn test_each_frame_is_counted_by_its_steps() -> TestResult {
    let dir = fresh("steps")?;
    let report: Value =
        serde_json::from_str(&run_allocs(&parsed(&dir, "1")?, &Stepped::default())?)?;
    assert_eq!(figure(&report, &["frames"])?, 20.0);
    assert_eq!(figure(&report, &["terms_per_frame", "p50"])?, 5.0);
    for step in ["read", "write", "commit"] {
        assert_eq!(
            figure(&report, &["allocations_per_frame", step, "p50"])?,
            1.0,
            "{step}"
        );
    }
    assert_eq!(
        figure(&report, &["allocations_per_frame", "total", "max"])?,
        3.0
    );
    assert_eq!(figure(&report, &["reallocations_per_frame", "min"])?, 6.0);
    assert_eq!(figure(&report, &["bytes_per_frame", "p99"])?, 300.0);
    assert_eq!(figure(&report, &["allocations_per_term_p50"])?, 0.6);
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

/// A frame of several relations names each subject once and its hubs at most once each.
#[test]
fn test_a_frame_of_several_relations_names_each_term_once() -> TestResult {
    let dir = fresh("several")?;
    let report: Value =
        serde_json::from_str(&run_allocs(&parsed(&dir, "10")?, &Stepped::default())?)?;
    let terms = figure(&report, &["terms_per_frame", "max"])?;
    // Ten subjects, the two predicates and the string, and at most the three hubs.
    assert!((14.0..=16.0).contains(&terms), "{terms}");
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

#[test]
fn test_a_run_takes_a_fresh_directory() -> TestResult {
    let dir = fresh("taken")?;
    std::fs::create_dir_all(&dir)?;
    let refused = run_allocs(&parsed(&dir, "1")?, &Stepped::default());
    assert!(
        matches!(&refused, Err(e) if matches!(&e.0, BenchErrorEnum::Usage(r) if r.contains("fresh directory"))),
        "{refused:?}"
    );
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
