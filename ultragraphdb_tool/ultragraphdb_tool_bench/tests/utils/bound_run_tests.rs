/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;

use serde_json::Value;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_bench::{BenchErrorEnum, run_wal_bench};

fn fresh(name: &str) -> TestResult<PathBuf> {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let dir = root.join(format!("bench-bound-{name}-{}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(dir)
}

fn args(dir: &std::path::Path, bound: u64) -> Vec<String> {
    [
        "--bound-walks",
        &bound.to_string(),
        "--repeats",
        "5",
        "--dir",
    ]
    .iter()
    .map(ToString::to_string)
    .chain([dir.display().to_string()])
    .collect()
}

/// Every walk is timed as many times as asked over the transactions bound, none of which a sweep ends.
#[test]
fn test_each_walk_is_timed_over_the_bound_transactions() -> TestResult {
    for bound in [0, 40] {
        let dir = fresh(&format!("walks{bound}"))?;
        let line = run_wal_bench(&args(&dir, bound))??;
        let report: Value = serde_json::from_str(&line)?;
        assert_eq!(
            report.get("run").and_then(Value::as_str),
            Some("bound_walks")
        );
        assert_eq!(report.get("bound").and_then(Value::as_u64), Some(bound));
        assert_eq!(report.get("repeats").and_then(Value::as_u64), Some(5));
        for walk in ["sweep_ns", "turn_ns", "answered_turn_ns"] {
            let at = |field: &str| {
                report
                    .get(walk)
                    .and_then(|times| times.get(field))
                    .and_then(Value::as_f64)
                    .context(field)
            };
            assert!(
                at("p50")? <= at("p99")? && at("p99")? <= at("max")?,
                "{line}"
            );
        }
    }
    Ok(())
}

/// A directory that exists is refused, so no run reads another's partition.
#[test]
fn test_a_directory_that_exists_is_refused() -> TestResult {
    let dir = fresh("exists")?;
    std::fs::create_dir_all(&dir)?;
    let refused = run_wal_bench(&args(&dir, 1))?;
    assert!(
        matches!(&refused, Err(e) if matches!(&e.0, BenchErrorEnum::Usage(r) if r.contains("exists"))),
        "{refused:?}"
    );
    Ok(())
}
