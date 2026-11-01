/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bench::{BenchErrorEnum, PipelineArgs};

fn args(line: &str) -> Vec<String> {
    line.split_whitespace().map(str::to_string).collect()
}

/// The reason a refused run gives.
fn refused(line: &str) -> String {
    match PipelineArgs::parse(&args(line)).unwrap_err().0 {
        BenchErrorEnum::Usage(reason) => reason,
        other => panic!("a usage error, not {other:?}"),
    }
}

#[test]
fn test_a_run_takes_its_rate_with_the_defaults_the_usage_names() {
    let a = PipelineArgs::parse(&args("--dir /data/w --rate 14500")).unwrap();
    assert_eq!(a.dir().to_str(), Some("/data/w"));
    assert_eq!(a.rate(), 14_500);
    // The defaults of the crate's usage: one relation, 30 s, 128 MiB, and no power-loss protection, the log's.
    assert_eq!(a.relations(), 1);
    assert_eq!(a.seconds(), 30);
    assert_eq!(a.cache_mb(), 128);
    assert!(!a.power_loss_protection());
}

#[test]
fn test_a_run_takes_every_flag() {
    let a = PipelineArgs::parse(&args(
        "--dir d --rate 50000 --relations 50 --seconds 5 --cache-mb 512 --plp on",
    ))
    .unwrap();
    assert_eq!(
        a,
        PipelineArgs::new("d".into(), 50_000, 50, 5, 512, true),
        "{a:?}"
    );
    let b = PipelineArgs::parse(&args("--dir d --rate 1 --plp off")).unwrap();
    assert!(!b.power_loss_protection());
}

#[test]
fn test_each_lower_bound_is_accepted() {
    // A rate, a relation count and a length of at least 1; a cache of 0 MiB holds nothing.
    let a = PipelineArgs::parse(&args(
        "--dir d --rate 1 --relations 1 --seconds 1 --cache-mb 0",
    ))
    .unwrap();
    assert_eq!(
        (a.rate(), a.relations(), a.seconds(), a.cache_mb()),
        (1, 1, 1, 0)
    );
}

#[test]
fn test_what_cannot_run_is_refused_naming_why() {
    assert_eq!(
        refused(""),
        "wal_bench --dir <path> --rate <commits/s> [options]"
    );
    assert_eq!(refused("--rate 5"), "--dir is required");
    assert_eq!(refused("--dir d"), "--rate is required");
    assert_eq!(refused("--dir d --rate 0"), "--rate must be at least 1");
    assert_eq!(
        refused("--dir d --rate 5 --relations 0"),
        "--relations must be at least 1"
    );
    assert_eq!(
        refused("--dir d --rate 5 --seconds 0"),
        "--seconds must be at least 1"
    );
    assert_eq!(
        refused("--dir d --rate fast"),
        "--rate takes a number, not fast"
    );
    assert_eq!(
        refused("--dir d --rate 5 --plp maybe"),
        "--plp is on or off, not maybe"
    );
    assert_eq!(
        refused("--dir d --rate 5 --durability process"),
        "unknown flag --durability"
    );
    assert_eq!(refused("--dir d --rate"), "--rate takes a value");
}
