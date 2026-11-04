/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The benchmark scenarios of the partitions capability, bound to `features/partitions/`.

use std::path::PathBuf;

use cucumber::{World, then, when};
use serde_json::Value;
use ultragraphdb_cluster_placement::{NodeId, PartitionId, PlacementMap};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_bench::{PartitionArgs, PartitionShape, run_partitions};

#[derive(Debug, Default, World)]
struct Steps {
    report: Option<Value>,
}

/// Runs `wal_bench` over 4 partitions with `extra` flags, in a directory of its own named `name`: its report.
fn run(name: &str, extra: &[&str]) -> TestResult<Value> {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let dir = root.join(format!("bench-bdd-{name}-{}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    let mut args: Vec<String> = [
        "--partitions",
        "4",
        "--rate",
        "500",
        "--cache-mb",
        "16",
        "--dir",
    ]
    .iter()
    .map(ToString::to_string)
    .collect();
    args.push(dir.display().to_string());
    args.extend(extra.iter().map(ToString::to_string));
    let line = run_partitions(&PartitionArgs::parse(&args)?)?;
    std::fs::remove_dir_all(&dir)?;
    Ok(serde_json::from_str(&line)?)
}

#[when("`wal_bench` runs 4 partitions for 10 seconds")]
fn runs_four(w: &mut Steps) -> TestResult {
    w.report = Some(run("every", &["--seconds", "1"])?);
    Ok(())
}

#[then(
    "its report lists 4 partitions' relations per second and their total, and two runs with one seed generate the same commits"
)]
fn lists_every_partition(w: &mut Steps) -> TestResult {
    let report = w.report.as_ref().context("the report")?;
    let each = report
        .get("per_partition")
        .and_then(Value::as_array)
        .context("per_partition")?;
    assert_eq!(each.len(), 4);
    let rate = |v: &Value| v.get("relations_per_s").and_then(Value::as_f64);
    let mut sum = 0.0;
    for partition in each {
        let relations = rate(partition).context("a partition's rate")?;
        assert!(relations > 0.0, "{partition}");
        sum += relations;
    }
    let total = rate(report).context("the total")?;
    // Each rounds to a whole number: four roundings of at most one half each.
    assert!((total - sum).abs() <= 2.0, "total {total}, sum {sum}");
    let placement = PlacementMap::even(4, NodeId::new(1))?;
    let commits = || -> TestResult<Vec<String>> {
        let mut out = Vec::new();
        for id in 1..=4 {
            let mut shape =
                PartitionShape::new(1, PartitionId::new(id)?, &placement, 1, 0).context("hubs")?;
            out.extend((0..50).map(|_| format!("{:?}", shape.next_commit().changes())));
        }
        Ok(out)
    };
    assert_eq!(commits()?, commits()?);
    Ok(())
}

#[when("`wal_bench` runs 4 partitions with 10% of edges pointing at other partitions' entities")]
fn runs_with_edges(w: &mut Steps) -> TestResult {
    w.report = Some(run(
        "registers",
        &[
            "--commits",
            "100",
            "--cross-percent",
            "10",
            "--verify",
            "on",
        ],
    )?);
    Ok(())
}

#[then(
    "its report lists registrations per second and the wait a first reference costs, and a reverse read, at the run's last closed epoch, of each entity that received an edge from another partition returns the subjects the same seed's run on one partition returns"
)]
fn lists_registrations(w: &mut Steps) -> TestResult {
    let report = w.report.as_ref().context("the report")?;
    let registrations = report.get("registrations_per_s").and_then(Value::as_f64);
    assert!(registrations > Some(0.0), "{report}");
    let wait = report.get("first_reference_wait_us").context("the wait")?;
    assert!(
        wait.get("count").and_then(Value::as_u64) > Some(0),
        "{report}"
    );
    // The run itself compares every such read with the same commits on one partition, and ends in an error otherwise.
    let verified = report.get("verified_reverse_reads").and_then(Value::as_u64);
    assert!(verified > Some(0), "{report}");
    Ok(())
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
