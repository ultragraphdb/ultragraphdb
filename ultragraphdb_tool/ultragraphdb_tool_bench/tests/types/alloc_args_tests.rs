/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_tool_bench::{AllocArgs, BenchError, BenchErrorEnum};

fn parsed(args: &[&str]) -> Result<AllocArgs, BenchError> {
    let owned: Vec<String> = args.iter().map(ToString::to_string).collect();
    AllocArgs::parse(&owned)
}

#[test]
fn test_the_defaults_and_every_flag() -> TestResult {
    let args = parsed(&["--dir", "/run"])?;
    assert_eq!(args.dir(), std::path::Path::new("/run"));
    assert_eq!(
        (args.frames(), args.relations(), args.hubs()),
        (10_000, 1, 1_000)
    );
    let every = parsed(&[
        "--dir",
        "/run",
        "--frames",
        "5",
        "--relations",
        "50",
        "--hubs",
        "3",
    ])?;
    assert_eq!(
        (every.frames(), every.relations(), every.hubs()),
        (5, 50, 3)
    );
    Ok(())
}

#[test]
fn test_what_cannot_run_is_refused_by_name() {
    for (args, named) in [
        (&["--frames", "5"][..], "--dir is required"),
        (
            &["--dir", "/run", "--frames", "0"][..],
            "--frames must be at least 1",
        ),
        (
            &["--dir", "/run", "--relations", "1001"][..],
            "--relations takes at most 1000",
        ),
        (
            &["--dir", "/run", "--hubs", "0"][..],
            "--hubs must be at least 1",
        ),
    ] {
        let refused = parsed(args);
        assert!(
            matches!(&refused, Err(e) if matches!(&e.0, BenchErrorEnum::Usage(r) if r.contains(named))),
            "{args:?}: {refused:?}"
        );
    }
    assert!(parsed(&[]).is_err(), "no arguments name no run");
}
