/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;

use serde_json::Value;
use ultragraphdb_tool_bench::{BenchErrorEnum, Command, run_apply};

fn fresh(name: &str) -> PathBuf {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let dir = root.join(format!("bench-apply-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// The report of an apply run of `flags` in a fresh directory, parsed.
fn report(name: &str, flags: &str) -> Value {
    let dir = fresh(name);
    let line = format!("apply --dir {} {flags}", dir.display());
    let args: Vec<String> = line.split_whitespace().map(str::to_string).collect();
    let Command::Apply(a) = Command::parse(&args).unwrap() else {
        panic!("an apply run");
    };
    let report = run_apply(&a).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    eprintln!("{report}");
    assert!(!report.contains('\n'), "one line: {report}");
    serde_json::from_str(&report).expect("the report is one JSON object")
}

fn u(v: &Value) -> u64 {
    v.as_u64().unwrap_or_else(|| panic!("{v} is no count"))
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("{v} is no number"))
}

/// A summary's p50 <= p99 <= max and mean <= max: what nearest-rank percentiles of any samples satisfy.
fn ordered(summary: &Value) {
    let (p50, p99, max) = (u(&summary["p50"]), u(&summary["p99"]), u(&summary["max"]));
    assert!(p50 <= p99 && p99 <= max, "{summary}");
    assert!(f(&summary["mean"]) <= max as f64, "{summary}");
}

/// The fields of the run that do not depend on how fast the machine is, and the invariants among the others.
fn check_run(r: &Value, relations: u64) {
    assert_eq!(r["run"], "apply");
    assert_eq!(r["offered_commits_per_s"], 10);
    assert_eq!(u(&r["relations_per_commit"]), relations);
    // The run lasts until the source stops at 1 s, and a little past it while the last batch applies.
    let secs = f(&r["seconds"]);
    assert!((1.0..5.0).contains(&secs), "{secs}");
    // At 10 a second, commits 1 to 9 are due before the source stops at 1 s, commit 10 at it: 9 on a machine that
    // keeps up, fewer only when the source thread is held off for the last of them.
    let offered = u(&r["commits_offered"]);
    assert!((5..=9).contains(&offered), "{offered}");
    let applied = u(&r["commits_applied"]);
    assert_eq!(applied, offered, "every commit sent is applied");
    assert_eq!(r["commit_errors"], 0);
    assert_eq!(
        r["source_waits_on_pending_limit"], 0,
        "9 commits never reach 256 in flight"
    );
    // The achieved rates are the applied commits and relations over the run's seconds, rounded.
    let achieved = f(&r["achieved_commits_per_s"]);
    assert!(
        (achieved - applied as f64 / secs).abs() <= 1.0,
        "{achieved}"
    );
    let achieved_relations = f(&r["achieved_relations_per_s"]);
    assert!(
        (achieved_relations - (applied * relations) as f64 / secs).abs() <= 1.0,
        "{achieved_relations}"
    );
    let (lookups, hits) = (u(&r["cache_lookups"]), u(&r["cache_hits"]));
    assert!(lookups > 0 && hits <= lookups, "{lookups} {hits}");
    let share = f(&r["cache_hit_share"]);
    assert!(
        (share - hits as f64 / lookups as f64).abs() <= 0.0005,
        "{share}"
    );
    assert!(u(&r["store_reads_found_nothing"]) <= u(&r["store_list_reads"]));
    assert!(f(&r["lists_read_per_relation"]) >= 0.0);
    let batches = u(&r["batches"]);
    assert!((1..=applied).contains(&batches), "{batches}");
    for summary in ["batch_commits", "batch_apply_us", "batch_write_call_us"] {
        ordered(&r[summary]);
    }
    // The batches' commits add up to the commits applied.
    let mean_commits = f(&r["batch_commits"]["mean"]);
    assert!((mean_commits * batches as f64 - applied as f64).abs() <= 0.05 * batches as f64);
    // The write calls are part of the batches' time, and the batches part of the run's.
    let write_share = f(&r["write_call_share"]);
    assert!((0.0..=1.0).contains(&write_share), "{write_share}");
    let busy = f(&r["apply_busy_share"]);
    assert!((0.0..=1.0).contains(&busy), "{busy}");
    assert!(
        u(&r["batch_write_call_us"]["max"]) <= u(&r["batch_apply_us"]["max"]),
        "a batch's write calls take part of its time"
    );
    // A run this small never reaches fjall's write pressure.
    assert_eq!(r["pressured_write_calls"], 0);
    assert_eq!(f(&r["stall_excess_ms"]), 0.0);
    assert!(u(&r["max_l0_tables"]) < 20 && u(&r["max_sealed_memtables"]) < 4);
    assert!(u(&r["write_baseline_ns_per_entry"]) > 0);
    // /proc exists on Linux alone; elsewhere the CPU and memory fields are null.
    for field in [
        "cpu_s",
        "cpu_cores",
        "apply_thread_cpu_s",
        "source_thread_cpu_s",
        "rss_kib",
        "rss_peak_kib",
    ] {
        assert_eq!(
            r[field].is_number(),
            cfg!(target_os = "linux"),
            "{field}: {}",
            r[field]
        );
    }
    // The files' total is the sum of its kinds, and its du bytes per relation written.
    let files = &r["files"];
    for measure in ["du_bytes", "apparent_bytes"] {
        let sum: u64 = ["data", "journal", "metadata"]
            .iter()
            .map(|kind| u(&files[kind][measure]))
            .sum();
        assert_eq!(u(&files["total"][measure]), sum, "{measure}");
    }
    let per_relation = f(&r["du_bytes_per_relation"]);
    let expected = u(&files["total"]["du_bytes"]) as f64 / (applied * relations) as f64;
    assert!(
        (per_relation - expected).abs() <= 0.05,
        "{per_relation} {expected}"
    );
}

#[test]
fn test_an_apply_run_without_rollups_reports_one_commit_per_batch() {
    let r = report(
        "batch-1",
        "--rate 10 --seconds 1 --relations 2 --durability filesystem --rollups off --cache-mb 1 --max-batch 1",
    );
    check_run(&r, 2);
    assert_eq!(r["durability"], "Filesystem");
    assert_eq!(r["rollups"], false);
    assert_eq!(r["max_batch"], 1);
    // One commit a batch: every batch is one commit.
    assert_eq!(r["batches"], r["commits_applied"]);
    assert_eq!(
        r["batch_commits"],
        serde_json::json!({"p50": 1, "p99": 1, "max": 1, "mean": 1.0})
    );
    // Without rollups no rollup writes: the applier's own writes at open and at the declarations are not counted.
    assert_eq!(r["rollup_keys"], 0);
    assert_eq!(r["rollup_write_calls"], 0);
    assert_eq!(f(&r["rollup_write_call_s"]), 0.0);
    assert!(r["rollup_thread_cpu_s"].is_null());
}

#[test]
fn test_an_apply_run_with_rollups_reports_their_keys_and_write_calls() {
    let r = report("rollups", "--rate 10 --seconds 1");
    check_run(&r, 1);
    assert_eq!(r["durability"], "Process");
    assert_eq!(r["rollups"], true);
    assert!(r["max_batch"].is_null());
    // A key counts as rolled up when its rollup wrote a batch, so there are at least as many rollup write calls.
    // How many keys the thread reaches in a second depends on the scheduler; on an idle machine it is 315.
    assert!(u(&r["rollup_write_calls"]) >= u(&r["rollup_keys"]), "{r}");
    assert!(f(&r["rollup_write_call_s"]) >= 0.0);
    assert_eq!(
        r["rollup_thread_cpu_s"].is_number(),
        cfg!(target_os = "linux")
    );
}

#[test]
fn test_an_apply_run_refuses_a_directory_that_exists() {
    let dir = fresh("exists");
    std::fs::create_dir_all(&dir).unwrap();
    let args: Vec<String> = format!("apply --dir {} --rate 10", dir.display())
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let Command::Apply(a) = Command::parse(&args).unwrap() else {
        panic!("an apply run");
    };
    let error = run_apply(&a).unwrap_err();
    let expected = format!(
        "{} exists; every run takes a fresh directory",
        dir.display()
    );
    assert!(
        matches!(&error.0, BenchErrorEnum::Usage(reason) if *reason == expected),
        "{error}"
    );
    assert!(
        std::fs::read_dir(&dir).unwrap().next().is_none(),
        "nothing written"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
