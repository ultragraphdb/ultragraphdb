/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;

use serde_json::Value;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_bench::{BenchErrorEnum, InboundArgs, run_inbound, run_wal_bench};

fn fresh(name: &str) -> TestResult<PathBuf> {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let dir = root.join(format!("bench-inbound-{name}-{}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(dir)
}

fn args(dir: &std::path::Path, partitions: &str, degrees: &str) -> Vec<String> {
    [
        "--inbound",
        "3",
        "--partitions",
        partitions,
        "--degrees",
        degrees,
        "--cache-mb",
        "16",
        "--dir",
    ]
    .iter()
    .map(ToString::to_string)
    .chain([dir.display().to_string()])
    .collect()
}

fn number(value: &Value, field: &str) -> TestResult<f64> {
    value.get(field).and_then(Value::as_f64).context(field)
}

/// Every edge commits, and each object's reverse read finds its in-degree over the partitions and on one: the fan-out
/// reads the object's referrer set once, a point read, and two lists of each partition it names, the object's
/// predicates and its reverse list, where one partition reads those two alone.
#[test]
fn test_each_object_is_read_over_the_partitions_its_referrer_set_names() -> TestResult {
    let dir = fresh("spread")?;
    let report: Value = serde_json::from_str(&run_inbound(&InboundArgs::parse(&args(
        &dir, "3", "1,4,9",
    ))?)?)?;
    assert_eq!(report.get("run").and_then(Value::as_str), Some("inbound"));
    assert_eq!(number(&report, "commits")?, 1.0 + 4.0 + 9.0);
    let rows = report
        .get("degrees")
        .and_then(Value::as_array)
        .context("degrees")?;
    let degrees: Vec<f64> = rows
        .iter()
        .map(|row| number(row, "in_degree"))
        .collect::<TestResult<_>>()?;
    assert_eq!(degrees, [1.0, 4.0, 9.0]);
    for row in rows {
        let asked = number(row, "partitions_asked")?;
        assert!(
            asked >= 1.0 && asked <= number(row, "in_degree")?.min(3.0),
            "{row}"
        );
        let spread = row.get("fanout").context("fanout")?;
        let single = row.get("one_partition").context("one_partition")?;
        assert_eq!(number(spread, "point_reads")?, 1.0, "{row}");
        assert_eq!(number(spread, "list_reads")?, 2.0 * asked, "{row}");
        assert_eq!(number(single, "point_reads")?, 0.0, "{row}");
        assert_eq!(number(single, "list_reads")?, 2.0, "{row}");
        for side in [spread, single] {
            let ns = side.get("ns").context("ns")?;
            assert!(number(ns, "p50")? > 0.0, "{row}");
            assert!(number(side, "cache_hits")? <= number(side, "list_reads")?);
        }
    }
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

/// Without a cache every list a read names is read from the store.
#[test]
fn test_without_a_cache_every_list_is_read_from_the_store() -> TestResult {
    let dir = fresh("uncached")?;
    let mut uncached = args(&dir, "2", "3");
    if let Some(mb) = uncached.iter_mut().find(|arg| *arg == "16") {
        *mb = "0".to_string();
    }
    let report: Value = serde_json::from_str(&run_inbound(&InboundArgs::parse(&uncached)?)?)?;
    let row = report
        .get("degrees")
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .context("the row")?;
    for side in ["fanout", "one_partition"] {
        let probes = row.get(side).context(side)?;
        assert_eq!(
            number(probes, "store_reads")?,
            number(probes, "list_reads")?,
            "{row}"
        );
    }
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}

/// A probe takes a fresh directory, and `--inbound` selects it over the run over partitions.
#[test]
fn test_a_probe_takes_a_fresh_directory_and_its_flag_selects_it() -> TestResult {
    let dir = fresh("select")?;
    let line = run_wal_bench(&args(&dir, "1", "2"))??;
    let report: Value = serde_json::from_str(&line)?;
    assert_eq!(report.get("run").and_then(Value::as_str), Some("inbound"));
    let refused = run_inbound(&InboundArgs::parse(&args(&dir, "1", "2"))?);
    assert!(
        matches!(&refused, Err(e) if matches!(&e.0, BenchErrorEnum::Usage(_))),
        "{refused:?}"
    );
    std::fs::remove_dir_all(&dir)?;
    Ok(())
}
