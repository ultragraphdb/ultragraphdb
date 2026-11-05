/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;

use serde_json::Value;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_bench::{BenchErrorEnum, PartitionArgs, run_partitions, run_wal_bench};

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
/// partitions registers only what a first write of a tenant must: `rdf:type` and the predicates homed elsewhere.
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

/// The partitions' entries of `report`.
fn partitions(report: &Value) -> TestResult<&Vec<Value>> {
    report
        .get("per_partition")
        .and_then(Value::as_array)
        .context("per_partition")
}

/// The total is every partition's: relations are commits times the commit's relations, the disk bytes each partition's
/// store and log, and every partition has taken the run's last close.
#[test]
fn test_the_report_adds_up() -> TestResult {
    let dir = fresh("adds")?;
    let report = run(&dir, &["--commits", "20", "--relations", "3"])?;
    let (mut commits, mut bytes) = (0, 0);
    let apparent = |v: Option<&Value>| -> TestResult<u64> {
        v.and_then(|size| size.get("apparent_bytes"))
            .and_then(Value::as_u64)
            .context("apparent_bytes")
    };
    for partition in partitions(&report)? {
        commits += number(partition, "commits")?;
        bytes += apparent(partition.get("store"))? + apparent(partition.get("wal"))?;
        assert_eq!(
            number(partition, "delivered")?,
            number(&report, "last_close")?
        );
    }
    assert_eq!(number(&report, "disk_bytes")?, bytes);
    let rate = report
        .get("relations_per_s")
        .and_then(Value::as_f64)
        .context("the rate")?;
    let seconds = report
        .get("seconds")
        .and_then(Value::as_f64)
        .context("the length")?;
    // The rate prints whole and the length to two places, so they multiply back to within a few relations.
    let relations = (commits * 3) as f64;
    assert!(
        (rate * seconds - relations).abs() < relations * 0.1 + 2.0,
        "{report}"
    );
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

/// A run to its length offers its rate: 100 commits a second for a second, each partition, not the unbounded rate a
/// broken pacer would offer nor a stalled one's none.
#[test]
fn test_a_run_offers_its_rate() -> TestResult {
    let dir = fresh("rate")?;
    let mut args = vec![
        "--partitions".to_string(),
        "2".to_string(),
        "--dir".to_string(),
        dir.display().to_string(),
    ];
    args.extend(
        ["--rate", "100", "--seconds", "1", "--cache-mb", "16"]
            .iter()
            .map(ToString::to_string),
    );
    let line = run_partitions(&PartitionArgs::parse(&args)?)?;
    let report: Value = serde_json::from_str(&line)?;
    for partition in partitions(&report)? {
        let commits = number(partition, "commits")? + number(partition, "refused")?;
        assert!((50..=130).contains(&commits), "{partition}");
    }
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

/// With no edge between the partitions a commit waits only for the predicates homed elsewhere, got once and then known:
/// commits in flight repeat a get until its reply lands, so how many wait is the clock's, yet not every commit does. No
/// reverse read is verified, since no entity received an edge from another partition.
#[test]
fn test_no_crossing_edges_stop_registering_and_verify_nothing() -> TestResult {
    let dir = fresh("none")?;
    let report = run(&dir, &["--commits", "400", "--verify", "on"])?;
    assert_eq!(number(&report, "verified_reverse_reads")?, 0);
    let waited = number(
        report.get("first_reference_wait_us").context("the wait")?,
        "count",
    )?;
    let commits = partitions(&report)?
        .iter()
        .map(|partition| number(partition, "commits"))
        .sum::<TestResult<u64>>()?;
    assert!(waited > 0, "a foreign predicate is got first: {report}");
    assert!(
        waited < commits,
        "known predicates wait for nothing: {report}"
    );
    assert!(
        number(&report, "registrations")? >= waited,
        "every commit that waited registered: {report}"
    );
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

/// With every edge crossing, every hub reached is verified, and a first reference takes real time.
#[test]
fn test_all_crossing_edges_verify_many_reads() -> TestResult {
    let dir = fresh("all")?;
    let report = run(
        &dir,
        &[
            "--commits",
            "20",
            "--cross-percent",
            "100",
            "--verify",
            "on",
        ],
    )?;
    assert!(number(&report, "verified_reverse_reads")? > 1, "{report}");
    let wait = report.get("first_reference_wait_us").context("the wait")?;
    assert!(
        number(wait, "max")? > 1,
        "the wait is the clock's: {report}"
    );
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

/// `--partitions` selects the run over partitions, and its absence the run of one log; arguments naming neither are the
/// outer refusal.
#[test]
fn test_the_flag_selects_the_run() -> TestResult {
    let refused = run_wal_bench(&["--partitions".to_string()]);
    assert!(
        refused.is_err(),
        "--partitions with nothing else names no run"
    );
    let dir = fresh("select")?;
    let args: Vec<String> = [
        "--rate",
        "50",
        "--seconds",
        "1",
        "--cache-mb",
        "16",
        "--dir",
    ]
    .iter()
    .map(ToString::to_string)
    .chain([dir.display().to_string()])
    .collect();
    let line = run_wal_bench(&args)??;
    let report: Value = serde_json::from_str(&line)?;
    assert_eq!(report.get("run").and_then(Value::as_str), Some("pipeline"));
    std::fs::remove_dir_all(&dir)?;
    let mut over = args;
    over.extend(["--partitions".to_string(), "1".to_string()]);
    let parted: Value = serde_json::from_str(&run_wal_bench(&over)??)?;
    assert_eq!(
        parted.get("run").and_then(Value::as_str),
        Some("partitions")
    );
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
