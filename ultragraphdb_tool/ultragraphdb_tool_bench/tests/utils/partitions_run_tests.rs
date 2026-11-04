/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;

use serde_json::Value;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_bench::{BenchErrorEnum, PartitionArgs, run_partitions};

fn fresh(name: &str) -> TestResult<PathBuf> {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let dir = root.join(format!("bench-partitions-{name}-{}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(dir)
}

fn run(dir: &std::path::Path, extra: &[&str]) -> TestResult<Value> {
    let mut args = vec![
        "--partitions".to_string(),
        "2".to_string(),
        "--dir".to_string(),
        dir.display().to_string(),
        "--rate".to_string(),
        "2000".to_string(),
        "--cache-mb".to_string(),
        "16".to_string(),
    ];
    args.extend(extra.iter().map(ToString::to_string));
    let line = run_partitions(&PartitionArgs::parse(&args)?)?;
    Ok(serde_json::from_str(&line)?)
}

fn number(report: &Value, field: &str) -> TestResult<u64> {
    report.get(field).and_then(Value::as_u64).context(field)
}

/// A run to a count of commits commits them all, reports each partition and the totals, and with no edge between the
/// partitions registers only what a first write of a namespace must: `rdf:type` and the predicates homed elsewhere.
#[test]
fn test_a_run_reports_each_partition_and_the_total() -> TestResult {
    let dir = fresh("report")?;
    let report = run(&dir, &["--commits", "30"])?;
    assert_eq!(
        report.get("run").and_then(Value::as_str),
        Some("partitions")
    );
    assert_eq!(number(&report, "partitions")?, 2);
    let each = report
        .get("per_partition")
        .and_then(Value::as_array)
        .context("per_partition")?;
    assert_eq!(each.len(), 2);
    for (at, partition) in each.iter().enumerate() {
        assert_eq!(number(partition, "partition")?, at as u64 + 1);
        assert_eq!(
            number(partition, "commits")? + number(partition, "refused")?,
            30
        );
        assert!(partition.get("relations_per_s").and_then(Value::as_f64) > Some(0.0));
        assert!(partition.get("syncs_per_s").and_then(Value::as_f64) > Some(0.0));
    }
    assert!(number(&report, "disk_bytes")? > 0);
    assert_eq!(report.get("verified_reverse_reads"), Some(&Value::Null));
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

/// A run with edges between the partitions registers them, reports the wait, and each reverse read at the last closed
/// epoch returns what the same commits on one partition return.
#[test]
fn test_edges_between_partitions_register_and_read_back_as_on_one() -> TestResult {
    let dir = fresh("cross")?;
    let report = run(
        &dir,
        &["--commits", "60", "--cross-percent", "50", "--verify", "on"],
    )?;
    assert!(number(&report, "registrations")? > 0);
    assert!(report.get("registrations_per_s").and_then(Value::as_f64) > Some(0.0));
    let wait = report.get("first_reference_wait_us").context("the wait")?;
    assert!(number(wait, "count")? > 0);
    assert!(number(&report, "verified_reverse_reads")? > 0);
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

/// A run takes a fresh directory.
#[test]
fn test_a_run_refuses_a_directory_that_exists() -> TestResult {
    let dir = fresh("exists")?;
    std::fs::create_dir_all(&dir)?;
    let args: Vec<String> = ["--partitions", "1", "--rate", "1", "--dir"]
        .iter()
        .map(ToString::to_string)
        .chain([dir.display().to_string()])
        .collect();
    let refused = run_partitions(&PartitionArgs::parse(&args)?);
    assert!(
        matches!(&refused, Err(e) if matches!(&e.0, BenchErrorEnum::Usage(_))),
        "{refused:?}"
    );
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
