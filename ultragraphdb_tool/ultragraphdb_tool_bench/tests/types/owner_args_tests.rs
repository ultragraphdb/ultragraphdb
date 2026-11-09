/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_tool_bench::{BenchError, BenchErrorEnum, OwnerArgs};

fn parsed(args: &[&str]) -> Result<OwnerArgs, BenchError> {
    let owned: Vec<String> = args.iter().map(ToString::to_string).collect();
    OwnerArgs::parse(&owned)
}

#[test]
fn test_the_defaults_and_every_flag() -> TestResult {
    let args = parsed(&["--owner-checks", "500", "--dir", "/run"])?;
    assert_eq!((args.checks(), args.epoch_ms()), (500, (2, 10)));
    assert_eq!(args.dir(), std::path::Path::new("/run"));
    let bounded = parsed(&["--owner-checks", "1", "--dir", "/run", "--epoch-ms", "1,5"])?;
    assert_eq!(bounded.epoch_ms(), (1, 5));
    Ok(())
}

#[test]
fn test_what_cannot_run_is_refused_by_name() {
    for (args, named) in [
        (&["--dir", "/run"][..], "--owner-checks is required"),
        (
            &["--owner-checks", "0", "--dir", "/run"][..],
            "--owner-checks must be at least 1",
        ),
        (&["--owner-checks", "5"][..], "--dir is required"),
        (
            &["--owner-checks", "5", "--dir", "/r", "--epoch-ms", "3"][..],
            "least,most",
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
