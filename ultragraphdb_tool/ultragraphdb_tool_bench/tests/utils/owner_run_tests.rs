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
    let dir = root.join(format!("bench-owner-{name}-{}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(dir)
}

fn args(dir: &std::path::Path, epochs: &str) -> Vec<String> {
    ["--owner-checks", "50", "--epoch-ms", epochs, "--dir"]
        .iter()
        .map(ToString::to_string)
        .chain([dir.display().to_string()])
        .collect()
}

/// Every check the production owner answers is timed: as many as asked, each of a positive time, ordered by rank.
#[test]
fn test_each_check_is_answered_and_timed() -> TestResult {
    let dir = fresh("checks")?;
    let line = run_wal_bench(&args(&dir, "1,5"))??;
    let report: Value = serde_json::from_str(&line)?;
    assert_eq!(
        report.get("run").and_then(Value::as_str),
        Some("owner_check")
    );
    assert_eq!(report.get("checks").and_then(Value::as_u64), Some(50));
    let at = |field: &str| {
        report
            .get("check_us")
            .and_then(|times| times.get(field))
            .and_then(Value::as_f64)
            .context(field)
    };
    assert!(
        at("p50")? <= at("p99")? && at("p99")? <= at("max")?,
        "{line}"
    );
    assert!(at("max")? > 0.0, "{line}");
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

/// Epoch bounds the sequencer refuses stop the run before the owner starts, and a used directory is refused.
#[test]
fn test_refused_bounds_and_a_used_directory_stop_the_run() -> TestResult {
    let dir = fresh("refused")?;
    let refused = run_wal_bench(&args(&dir, "5,1"))?;
    assert!(
        matches!(&refused, Err(e) if matches!(&e.0, BenchErrorEnum::Usage(_))),
        "{refused:?}"
    );
    let used = run_wal_bench(&args(&dir, "1,5"))?;
    assert!(
        matches!(&used, Err(e) if matches!(&e.0, BenchErrorEnum::Usage(r) if r.contains("fresh directory"))),
        "{used:?}"
    );
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
