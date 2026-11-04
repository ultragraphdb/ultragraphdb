/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_tool_bench::{BenchErrorEnum, PartitionArgs};

fn parsed(args: &[&str]) -> Result<PartitionArgs, ultragraphdb_tool_bench::BenchError> {
    let owned: Vec<String> = args.iter().map(ToString::to_string).collect();
    PartitionArgs::parse(&owned)
}

const REQUIRED: [&str; 6] = ["--partitions", "4", "--dir", "/run", "--rate", "100"];

#[test]
fn test_the_required_flags_and_the_defaults() -> TestResult {
    let args = parsed(&REQUIRED)?;
    assert_eq!((args.partitions(), args.rate()), (4, 100));
    assert_eq!(args.dir(), std::path::Path::new("/run"));
    assert_eq!(
        (args.relations(), args.seconds(), args.commits()),
        (1, 30, 0)
    );
    assert_eq!(
        (args.cross_percent(), args.seed(), args.cache_mb()),
        (0, 1, 128)
    );
    assert!(!args.verify());
    Ok(())
}

#[test]
fn test_every_flag_is_taken() -> TestResult {
    let mut all = REQUIRED.to_vec();
    all.extend([
        "--relations",
        "3",
        "--seconds",
        "9",
        "--commits",
        "7",
        "--cross-percent",
        "100",
        "--seed",
        "42",
        "--cache-mb",
        "16",
        "--verify",
        "on",
    ]);
    let args = parsed(&all)?;
    assert_eq!(
        (args.relations(), args.seconds(), args.commits()),
        (3, 9, 7)
    );
    assert_eq!(
        (args.cross_percent(), args.seed(), args.cache_mb()),
        (100, 42, 16)
    );
    assert!(args.verify());
    Ok(())
}

#[test]
fn test_what_cannot_run_is_refused_by_name() {
    for (extra, named) in [
        (vec!["--cross-percent", "101"], "--cross-percent"),
        (vec!["--verify", "maybe"], "--verify"),
    ] {
        let mut args = REQUIRED.to_vec();
        args.extend(extra);
        let refused = parsed(&args).map(|_| ());
        assert!(
            matches!(&refused, Err(e) if matches!(&e.0, BenchErrorEnum::Usage(r) if r.contains(named))),
            "{refused:?}"
        );
    }
    assert!(parsed(&[]).is_err(), "no arguments name no run");
    assert!(
        parsed(&["--partitions", "0", "--dir", "/run", "--rate", "1"]).is_err(),
        "a run has a partition"
    );
}
