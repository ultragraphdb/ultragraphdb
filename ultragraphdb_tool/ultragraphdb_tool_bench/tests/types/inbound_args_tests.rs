/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_tool_bench::{BenchError, BenchErrorEnum, InboundArgs};

fn parsed(args: &[&str]) -> Result<InboundArgs, BenchError> {
    let owned: Vec<String> = args.iter().map(ToString::to_string).collect();
    InboundArgs::parse(&owned)
}

const REQUIRED: [&str; 6] = ["--inbound", "50", "--partitions", "12", "--dir", "/run"];

#[test]
fn test_the_required_flags_and_the_defaults() -> TestResult {
    let args = parsed(&REQUIRED)?;
    assert_eq!((args.reads(), args.partitions()), (50, 12));
    assert_eq!(args.dir(), std::path::Path::new("/run"));
    assert_eq!(args.degrees(), [1, 20, 1_000]);
    assert_eq!(args.cache_mb(), 128);
    Ok(())
}

#[test]
fn test_every_flag_is_taken() -> TestResult {
    let mut all = REQUIRED.to_vec();
    all.extend(["--degrees", "3,1,7", "--cache-mb", "0"]);
    let args = parsed(&all)?;
    assert_eq!(args.degrees(), [3, 1, 7]);
    assert_eq!(args.cache_mb(), 0);
    Ok(())
}

#[test]
fn test_what_cannot_run_is_refused_by_name() {
    for (degrees, named) in [
        ("0", "--degrees"),
        ("1,x", "--degrees"),
        ("100001", "--degrees"),
        ("1,2,3,4,5,6,7,8,9", "at most 8"),
    ] {
        let mut args = REQUIRED.to_vec();
        args.extend(["--degrees", degrees]);
        let refused = parsed(&args).map(|_| ());
        assert!(
            matches!(&refused, Err(e) if matches!(&e.0, BenchErrorEnum::Usage(r) if r.contains(named))),
            "{degrees}: {refused:?}"
        );
    }
    assert!(parsed(&[]).is_err(), "no arguments name no run");
    for missing in ["--inbound", "--partitions", "--dir"] {
        let at = REQUIRED.iter().position(|flag| *flag == missing);
        let without: Vec<&str> = REQUIRED
            .iter()
            .enumerate()
            .filter(|(i, _)| at.is_none_or(|at| *i != at && *i != at + 1))
            .map(|(_, flag)| *flag)
            .collect();
        assert!(parsed(&without).is_err(), "{missing} is required");
    }
    let mut none = REQUIRED.to_vec();
    none[1] = "0";
    assert!(parsed(&none).is_err(), "a run makes at least one read");
}

#[test]
fn test_eight_in_degrees_are_taken() -> TestResult {
    let mut args = REQUIRED.to_vec();
    args.extend(["--degrees", "1,2,3,4,5,6,7,100000"]);
    assert_eq!(parsed(&args)?.degrees().len(), 8);
    Ok(())
}
