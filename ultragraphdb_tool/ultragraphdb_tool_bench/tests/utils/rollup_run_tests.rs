/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;

use serde_json::Value;
use ultragraphdb_tool_bench::{BenchErrorEnum, RollupArgs, run_rollup};

fn fresh(name: &str) -> PathBuf {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let dir = root.join(format!("bench-rollup-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn u(v: &Value) -> u64 {
    v.as_u64().unwrap_or_else(|| panic!("{v} is no count"))
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("{v} is no number"))
}

fn versions(keys: u64, above: u64, at_or_below: u64) -> Value {
    serde_json::json!({
        "keys": keys,
        "versions_above_discard": above,
        "versions_at_or_below_discard": at_or_below,
    })
}

#[test]
fn test_a_rollup_run_reports_the_versions_its_rollups_and_compaction_leave() {
    let dir = fresh("small");
    let report = run_rollup(&RollupArgs::new(dir.clone(), 2, 3, 2)).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!report.contains('\n'), "one line: {report}");
    let r: Value = serde_json::from_str(&report).expect("the report is one JSON object");
    assert_eq!(r["run"], "rollup");
    assert_eq!(
        (
            u(&r["hubs"]),
            u(&r["versions_target"]),
            u(&r["relations_per_commit"])
        ),
        (2, 3, 2)
    );
    // 2 hubs of 3 versions at 2 relations a commit: ceil(6 / 2) = 3 commits, 6 relations.
    assert_eq!(
        (u(&r["load"]["commits"]), u(&r["load"]["relations"])),
        (3, 6)
    );
    assert!(f(&r["load"]["commits_per_s"]) > 0.0);
    // By hand, from the seed-1 xorshift: the commits link r1, r2 to h1; r3 to h1 and r4 to h0; r5 to h1 and r6 to
    // h0. A commit writes one delta per hub it links, so h1 holds 3 deltas and h0 2; each hub is rolled up once.
    assert_eq!(u(&r["rollup"]["keys"]), 2);
    // Before compaction every version is at or below the discard timestamp: 5 deltas and 2 rollups on the reverse
    // lists, one version for each of the 6 forward edges. Compaction leaves each reverse list its rollup alone.
    assert_eq!(r["reverse_before"], versions(2, 0, 7));
    assert_eq!(r["reverse_after"], versions(2, 0, 2));
    assert_eq!(r["forward_before"], versions(6, 0, 6));
    assert_eq!(r["forward_after"], versions(6, 0, 6));
    assert_eq!(u(&r["compaction"]["reverse_collected"]), 5);
    assert_eq!(u(&r["compaction"]["forward_collected"]), 0);
    assert!(u(&r["discard_ts"]) > 0);
    // Over one rollup time, 2 keys and 5 deltas: the deltas a second are 2.5 times the keys, within rounding.
    let (keys_per_s, deltas_per_s) = (
        f(&r["rollup"]["keys_per_s"]),
        f(&r["rollup"]["deltas_per_s"]),
    );
    assert!(keys_per_s > 0.0);
    assert!(
        (deltas_per_s - 2.5 * keys_per_s).abs() <= 2.5,
        "{keys_per_s} {deltas_per_s}"
    );
    assert!(f(&r["rollup"]["seconds"]) >= 0.0 && f(&r["compaction"]["seconds"]) >= 0.0);
    for (object, field) in [
        (&r["rollup"], "cpu_s"),
        (&r["compaction"], "cpu_s"),
        (&r, "rss_kib"),
        (&r, "rss_peak_kib"),
    ] {
        assert_eq!(
            object[field].is_number(),
            cfg!(target_os = "linux"),
            "{field}"
        );
    }
    for files in [
        "files_loaded",
        "files_rolled_up",
        "files_compacted",
        "files_closed",
    ] {
        for measure in ["du_bytes", "apparent_bytes"] {
            let sum: u64 = ["data", "journal", "metadata"]
                .iter()
                .map(|kind| u(&r[files][kind][measure]))
                .sum();
            assert_eq!(u(&r[files]["total"][measure]), sum, "{files} {measure}");
        }
    }
    // The closed store's du bytes over the 6 relations.
    let per_relation = f(&r["du_bytes_per_relation_closed"]);
    let expected = u(&r["files_closed"]["total"]["du_bytes"]) as f64 / 6.0;
    assert!(
        (per_relation - expected).abs() <= 0.05,
        "{per_relation} {expected}"
    );
}

#[test]
fn test_a_rollup_run_refuses_a_directory_that_exists() {
    let dir = fresh("exists");
    std::fs::create_dir_all(&dir).unwrap();
    let error = run_rollup(&RollupArgs::new(dir.clone(), 2, 3, 2)).unwrap_err();
    let expected = format!(
        "{} exists; every run takes a fresh directory",
        dir.display()
    );
    assert!(
        matches!(&error.0, BenchErrorEnum::Usage(reason) if *reason == expected),
        "{error}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
