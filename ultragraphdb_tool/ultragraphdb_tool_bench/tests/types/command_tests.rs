/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_kv::Durability;
use ultragraphdb_tool_bench::{BenchErrorEnum, Command};

fn args(line: &str) -> Vec<String> {
    line.split_whitespace().map(str::to_string).collect()
}

#[test]
fn test_an_apply_run_takes_its_rate_and_shape_with_defaults() {
    let Command::Apply(a) = Command::parse(&args("apply --dir /data/s --rate 14500")).unwrap()
    else {
        panic!("an apply run");
    };
    assert_eq!(a.dir().to_str(), Some("/data/s"));
    assert_eq!(a.rate(), 14_500);
    assert_eq!(a.relations(), 1);
    assert_eq!(a.seconds(), 30);
    assert_eq!(a.durability(), Durability::Process);
    assert!(a.rollups());
    assert_eq!(a.cache_mb(), 128);
    assert_eq!(a.max_batch(), None);
    let Command::Apply(b) = Command::parse(&args(
        "apply --dir d --rate 50000 --relations 50 --seconds 5 --durability filesystem --rollups off --cache-mb 512",
    ))
    .unwrap() else {
        panic!("an apply run");
    };
    assert_eq!(
        (
            b.rate(),
            b.relations(),
            b.seconds(),
            b.durability(),
            b.rollups(),
            b.cache_mb()
        ),
        (50_000, 50, 5, Durability::Filesystem, false, 512)
    );
    let Command::Apply(c) =
        Command::parse(&args("apply --dir d --rate 5 --max-batch 256")).unwrap()
    else {
        panic!("an apply run");
    };
    assert_eq!(c.max_batch(), Some(256));
}

#[test]
fn test_a_rollup_run_takes_its_predicate_size() {
    let Command::Rollup(r) =
        Command::parse(&args("rollup --dir d --hubs 7 --versions 3 --relations 2")).unwrap()
    else {
        panic!("a rollup run");
    };
    assert_eq!((r.hubs(), r.versions(), r.relations()), (7, 3, 2));
    assert_eq!(r.dir().to_str(), Some("d"));
}

#[test]
fn test_a_rollup_run_defaults_to_the_dgraph_runs_predicate() {
    // The defaults the usage names: 10,000 hubs, 1,000 versions, 50 relations a commit.
    let Command::Rollup(r) = Command::parse(&args("rollup --dir d")).unwrap() else {
        panic!("a rollup run");
    };
    assert_eq!((r.hubs(), r.versions(), r.relations()), (10_000, 1_000, 50));
}

#[test]
fn test_each_lower_bound_is_accepted() {
    // A rate of 0 offers nothing; a cache of 0 MiB holds nothing; every other count is at least 1.
    let Command::Apply(a) = Command::parse(&args(
        "apply --dir d --rate 0 --relations 1 --seconds 1 --cache-mb 0 --max-batch 1",
    ))
    .unwrap() else {
        panic!("an apply run");
    };
    assert_eq!(
        (
            a.rate(),
            a.relations(),
            a.seconds(),
            a.cache_mb(),
            a.max_batch()
        ),
        (0, 1, 1, 0, Some(1))
    );
    let Command::Rollup(r) =
        Command::parse(&args("rollup --dir d --hubs 1 --versions 1 --relations 1")).unwrap()
    else {
        panic!("a rollup run");
    };
    assert_eq!((r.hubs(), r.versions(), r.relations()), (1, 1, 1));
}

#[test]
fn test_the_largest_number_is_accepted_and_one_past_it_refused() {
    let Command::Apply(a) =
        Command::parse(&args("apply --dir d --rate 18446744073709551615")).unwrap()
    else {
        panic!("an apply run");
    };
    assert_eq!(a.rate(), u64::MAX);
    assert_eq!(
        Command::parse(&args("apply --dir d --rate 18446744073709551616"))
            .unwrap_err()
            .to_string(),
        "usage: --rate takes a number, not 18446744073709551616"
    );
}

#[test]
fn test_a_repeated_flag_takes_its_last_value() {
    let Command::Apply(a) =
        Command::parse(&args("apply --dir a --rate 5 --rate 7 --dir b")).unwrap()
    else {
        panic!("an apply run");
    };
    assert_eq!((a.rate(), a.dir().to_str()), (7, Some("b")));
}

#[test]
fn test_the_flags_and_values_are_given_explicitly() {
    let Command::Apply(on) = Command::parse(&args(
        "apply --dir d --rate 5 --durability process --rollups on",
    ))
    .unwrap() else {
        panic!("an apply run");
    };
    assert_eq!((on.durability(), on.rollups()), (Durability::Process, true));
}

#[test]
fn test_what_cannot_run_is_refused_with_the_reason() {
    let refused = |line: &str| Command::parse(&args(line)).unwrap_err().to_string();
    assert_eq!(
        refused(""),
        "usage: store_bench apply|rollup --dir <path> [options]"
    );
    assert_eq!(refused("load --dir d"), "usage: unknown run load");
    assert_eq!(refused("apply --rate 5"), "usage: --dir is required");
    assert_eq!(refused("apply --dir d"), "usage: --rate is required");
    assert_eq!(
        refused("apply --dir d --rate x"),
        "usage: --rate takes a number, not x"
    );
    assert_eq!(
        refused("apply --dir d --rate 5 --relations 0"),
        "usage: --relations must be at least 1"
    );
    assert_eq!(
        refused("apply --dir d --rate 5 --durability disk"),
        "usage: --durability is process or filesystem, not disk"
    );
    assert_eq!(
        refused("apply --dir d --rate 5 --rollups maybe"),
        "usage: --rollups is on or off, not maybe"
    );
    assert_eq!(
        refused("apply --dir d --rate 5 --colour red"),
        "usage: unknown flag --colour"
    );
    assert_eq!(
        refused("apply --dir d --rate"),
        "usage: --rate takes a value"
    );
    assert_eq!(
        refused("rollup --dir d --hubs 0"),
        "usage: --hubs must be at least 1"
    );
    for (line, reason) in [
        ("--dir d", "unknown run --dir"),
        ("Apply --dir d --rate 5", "unknown run Apply"),
        (
            "apply --dir d --rate 5 --seconds 0",
            "--seconds must be at least 1",
        ),
        (
            "apply --dir d --rate 5 --max-batch 0",
            "--max-batch must be at least 1",
        ),
        ("apply --dir d --rate -5", "--rate takes a number, not -5"),
        (
            "apply --dir d --rate 5 --cache-mb 1.5",
            "--cache-mb takes a number, not 1.5",
        ),
        ("apply --dir d --rate 5 --hubs 3", "unknown flag --hubs"),
        ("rollup --rate 5", "unknown flag --rate"),
        ("rollup --hubs 3", "--dir is required"),
        (
            "rollup --dir d --versions 0",
            "--versions must be at least 1",
        ),
        (
            "rollup --dir d --relations 0",
            "--relations must be at least 1",
        ),
        ("rollup --dir d --hubs", "--hubs takes a value"),
        ("apply --dir", "--dir takes a value"),
        // A value is the next argument, whatever it looks like.
        ("apply --dir --rate 5", "unknown flag 5"),
        (
            "apply --dir d --rate 5 --durability",
            "--durability takes a value",
        ),
        (
            "apply --dir d --rate 5 --rollups ON",
            "--rollups is on or off, not ON",
        ),
    ] {
        assert_eq!(refused(line), format!("usage: {reason}"), "{line}");
    }
}

#[test]
fn test_a_refusal_is_the_usage_variant() {
    let error = Command::parse(&args("apply --dir d")).unwrap_err();
    assert!(
        matches!(&error.0, BenchErrorEnum::Usage(reason) if reason == "--rate is required"),
        "{error}"
    );
}
