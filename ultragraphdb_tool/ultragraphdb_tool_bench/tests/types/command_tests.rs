/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_kv::Durability;
use ultragraphdb_tool_bench::Command;

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
        Command::parse(&args("rollup --dir d --hubs 10000 --versions 1000")).unwrap()
    else {
        panic!("a rollup run");
    };
    assert_eq!((r.hubs(), r.versions(), r.relations()), (10_000, 1_000, 50));
    assert_eq!(r.dir().to_str(), Some("d"));
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
}
