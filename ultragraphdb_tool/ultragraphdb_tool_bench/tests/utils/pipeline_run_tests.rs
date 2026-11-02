/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;

use serde_json::Value;
use ultragraphdb_tool_bench::{BenchErrorEnum, PipelineArgs, run_pipeline};

/// A write-ahead log block: every round is written as its frame padded to whole blocks of 4,096 bytes (the change's
/// design, "The WAL").
const BLOCK: u64 = 4_096;

fn fresh(name: &str) -> PathBuf {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let dir = root.join(format!("bench-pipeline-{name}-{}", std::process::id()));
    // A previous run's directory, if one is left.
    if let Err(e) = std::fs::remove_dir_all(&dir) {
        assert_eq!(
            e.kind(),
            std::io::ErrorKind::NotFound,
            "cannot clear {}: {e}",
            dir.display()
        );
    }
    dir
}

fn parse(dir: &std::path::Path, flags: &str) -> PipelineArgs {
    let line = format!("--dir {} {flags}", dir.display());
    let args: Vec<String> = line.split_whitespace().map(str::to_string).collect();
    PipelineArgs::parse(&args).unwrap()
}

/// The report of a pipeline run of `flags` in a fresh directory, parsed.
fn report(name: &str, flags: &str) -> Value {
    let dir = fresh(name);
    let report = run_pipeline(&parse(&dir, flags)).unwrap();
    std::fs::remove_dir_all(&dir).expect("removing the test's directory");
    eprintln!("{report}");
    assert!(!report.contains('\n'), "one line: {report}");
    serde_json::from_str(&report).expect("the report is one JSON object")
}

fn u(v: &Value) -> u64 {
    v.as_u64()
        .ok_or_else(|| format!("{v} is no count"))
        .expect("a count")
}

fn f(v: &Value) -> f64 {
    v.as_f64()
        .ok_or_else(|| format!("{v} is no number"))
        .expect("a number")
}

#[test]
fn test_a_pipeline_run_commits_every_commit_offered_in_rounds_of_the_log() {
    let r = report("short", "--rate 10 --seconds 1 --relations 2 --cache-mb 1");
    assert_eq!(r["run"], "pipeline");
    assert_eq!(r["offered_commits_per_s"], 10);
    assert_eq!(r["relations_per_commit"], 2);
    assert_eq!(r["power_loss_protection"], false);
    // The run lasts until the source stops at 1 s, and a little past it while the last round syncs.
    let secs = f(&r["seconds"]);
    assert!((1.0..5.0).contains(&secs), "{secs}");
    // At 10 a second, commits 1 to 9 are due before the source stops at 1 s, commit 10 at it: 9 on a machine that
    // keeps up, fewer only when the source thread is held off for the last of them.
    let offered = u(&r["commits_offered"]);
    assert!((5..=9).contains(&offered), "{offered}");
    // Each commit writes new nodes, so none conflicts with another: every one commits.
    assert_eq!(u(&r["commits_committed"]), offered);
    assert_eq!(r["commits_aborted"], 0);
    let achieved = f(&r["achieved_commits_per_s"]);
    assert!(
        (achieved - offered as f64 / secs).abs() <= 1.0,
        "{achieved}"
    );
    let relations = f(&r["achieved_relations_per_s"]);
    assert!(
        (relations - (2 * offered) as f64 / secs).abs() <= 1.0,
        "{relations}"
    );
    // A round holds at least one commit, and each sync completes one round.
    let rounds = u(&r["rounds"]);
    assert!((1..=offered).contains(&rounds), "{rounds}");
    let per_round = f(&r["commits_per_round"]);
    assert!(
        (per_round - offered as f64 / rounds as f64).abs() <= 0.005,
        "{per_round}"
    );
    let round = &r["round_sync_apply_us"];
    assert!(u(&round["p50"]) <= u(&round["p99"]) && u(&round["p99"]) <= u(&round["max"]));
    assert!(f(&round["mean"]) <= u(&round["max"]) as f64);
    // The log keeps every round, none trimmed: whole blocks, at least one a round.
    let wal = u(&r["wal"]["segments"]["apparent_bytes"]);
    assert_eq!(wal % BLOCK, 0, "{wal}");
    assert!(wal >= rounds * BLOCK, "{wal} {rounds}");
    assert!(u(&r["wal"]["superblock"]["apparent_bytes"]) > 0);
    // The journal held the declarations and the hubs before the run, and the commits' entries after it.
    let (before, after) = (
        u(&r["journal_written"]["before_run"]),
        u(&r["journal_written"]["after_run"]),
    );
    assert!(0 < before && before < after, "{before} {after}");
    // The bytes per relation are the log's and the journal's run bytes over the relations committed.
    let per_relation = |bytes: u64| bytes as f64 / (2 * offered) as f64;
    assert!((f(&r["wal_bytes_per_relation"]) - per_relation(wal)).abs() <= 0.05);
    assert!((f(&r["journal_bytes_per_relation"]) - per_relation(after - before)).abs() <= 0.05);
    let ratio = f(&r["wal_to_journal"]);
    assert!(
        (ratio - wal as f64 / (after - before) as f64).abs() <= 0.005,
        "{ratio}"
    );
    // The store's total is the sum of its kinds.
    let store = &r["store"];
    for measure in ["du_bytes", "apparent_bytes"] {
        let sum: u64 = ["data", "journal", "metadata"]
            .iter()
            .map(|kind| u(&store[kind][measure]))
            .sum();
        assert_eq!(u(&store["total"][measure]), sum, "{measure}");
    }
}

#[test]
fn test_a_pipeline_run_with_power_loss_protection_says_so() {
    let r = report("plp", "--rate 10 --seconds 1 --cache-mb 1 --plp on");
    assert_eq!(r["power_loss_protection"], true);
    assert_eq!(r["relations_per_commit"], 1);
    assert_eq!(u(&r["commits_committed"]), u(&r["commits_offered"]));
}

#[test]
fn test_a_pipeline_run_refuses_a_directory_that_exists() {
    let dir = fresh("exists");
    std::fs::create_dir_all(&dir).unwrap();
    let error = run_pipeline(&parse(&dir, "--rate 10")).unwrap_err();
    let expected = format!(
        "{} exists; every run takes a fresh directory",
        dir.display()
    );
    assert!(
        matches!(&error.0, BenchErrorEnum::Usage(reason) if *reason == expected),
        "{error}"
    );
    // Nothing was written into it.
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
    std::fs::remove_dir_all(&dir).expect("removing the test's directory");
}
