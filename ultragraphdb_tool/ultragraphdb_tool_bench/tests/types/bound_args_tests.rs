/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_tool_bench::{BenchError, BenchErrorEnum, BoundArgs};

fn parsed(args: &[&str]) -> Result<BoundArgs, BenchError> {
    let owned: Vec<String> = args.iter().map(ToString::to_string).collect();
    BoundArgs::parse(&owned)
}

#[test]
fn test_the_defaults_and_every_flag() -> TestResult {
    let args = parsed(&["--bound-walks", "10000", "--dir", "/run"])?;
    assert_eq!((args.bound(), args.repeats()), (10_000, 1_000));
    assert_eq!(args.dir(), std::path::Path::new("/run"));
    let none = parsed(&["--bound-walks", "0", "--dir", "/run", "--repeats", "7"])?;
    assert_eq!((none.bound(), none.repeats()), (0, 7));
    Ok(())
}

#[test]
fn test_what_cannot_run_is_refused_by_name() {
    for (args, named) in [
        (&["--dir", "/run"][..], "--bound-walks is required"),
        (
            &["--bound-walks", "1000001", "--dir", "/run"][..],
            "--bound-walks takes at most 1000000",
        ),
        (&["--bound-walks", "5"][..], "--dir is required"),
        (
            &["--bound-walks", "5", "--dir", "/r", "--repeats", "0"][..],
            "--repeats must be at least 1",
        ),
        (&[][..], "wal_bench --bound-walks"),
    ] {
        let refused = parsed(args);
        assert!(
            matches!(&refused, Err(e) if matches!(&e.0, BenchErrorEnum::Usage(r) if r.contains(named))),
            "{args:?}: {refused:?}"
        );
    }
}
