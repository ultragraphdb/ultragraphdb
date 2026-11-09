/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::ReadLevel;
use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_tool_bench::{BenchError, BenchErrorEnum, ClientArgs, ClientOp};

fn parsed(args: &[&str]) -> Result<ClientArgs, BenchError> {
    let owned: Vec<String> = args.iter().map(ToString::to_string).collect();
    ClientArgs::parse(&owned)
}

const REQUIRED: [&str; 6] = [
    "--address",
    "127.0.0.1:7080",
    "--password-file",
    "/data/config/bootstrap.secret",
    "--op",
    "direct",
];

fn refused_naming(args: &[&str], named: &str) -> bool {
    matches!(parsed(args), Err(e) if matches!(&e.0, BenchErrorEnum::Usage(r) if r.contains(named)))
}

#[test]
fn test_the_required_flags_and_the_defaults() -> TestResult {
    let args = parsed(&REQUIRED)?;
    assert_eq!(args.address(), "127.0.0.1:7080".parse()?);
    assert_eq!(
        args.password_file(),
        std::path::Path::new("/data/config/bootstrap.secret")
    );
    assert_eq!(args.op(), ClientOp::Direct);
    assert_eq!(
        (args.connections(), args.seconds(), args.relations()),
        (1, 30, 1)
    );
    assert!(
        args.session(),
        "commits go through a session unless told otherwise"
    );
    assert_eq!(
        (args.partitions(), args.hubs(), args.subjects()),
        (1, 1_000, 1_000)
    );
    assert_eq!((args.cluster(), args.seed()), (1, 1));
    assert_eq!((args.level(), args.limit()), (ReadLevel::Snapshot, 0));
    assert_eq!(
        (
            args.predicates(),
            args.fresh(),
            args.writes(),
            args.value_bytes()
        ),
        (8, false, 16, 1)
    );
    Ok(())
}

#[test]
fn test_every_flag_is_taken() -> TestResult {
    let mut all = REQUIRED.to_vec();
    all.extend([
        "--op",
        "read",
        "--connections",
        "16",
        "--seconds",
        "5",
        "--relations",
        "50",
        "--session",
        "off",
        "--partitions",
        "4",
        "--hubs",
        "40",
        "--subjects",
        "200",
        "--cluster",
        "7",
        "--seed",
        "9",
        "--level",
        "monotonic",
        "--limit",
        "3",
        "--predicates",
        "12",
        "--fresh",
        "on",
        "--writes",
        "40",
        "--value-bytes",
        "1024",
    ]);
    let args = parsed(&all)?;
    assert_eq!(args.op(), ClientOp::Read);
    assert_eq!(
        (args.connections(), args.seconds(), args.relations()),
        (16, 5, 50)
    );
    assert!(!args.session());
    assert_eq!(
        (args.partitions(), args.hubs(), args.subjects()),
        (4, 40, 200)
    );
    assert_eq!((args.cluster(), args.seed()), (7, 9));
    assert_eq!((args.level(), args.limit()), (ReadLevel::MonotonicFast, 3));
    assert_eq!(
        (
            args.predicates(),
            args.fresh(),
            args.writes(),
            args.value_bytes()
        ),
        (12, true, 40, 1_024)
    );
    assert_eq!(
        parsed(&[&REQUIRED[..4], &["--op", "transaction"]].concat())?.op(),
        ClientOp::Transaction
    );
    Ok(())
}

#[test]
fn test_each_op_is_named_as_it_is_taken() -> TestResult {
    for (op, name) in [
        (ClientOp::Direct, "direct"),
        (ClientOp::Transaction, "transaction"),
        (ClientOp::Read, "read"),
        (ClientOp::Scan, "scan"),
        (ClientOp::Span, "span"),
        (ClientOp::Stall, "stall"),
        (ClientOp::ListRead, "list-read"),
        (ClientOp::Hot, "hot"),
    ] {
        assert_eq!(op.name(), name);
        assert_eq!(ClientOp::of_name(name), Some(op));
        assert_eq!(
            parsed(&[&REQUIRED[..4], &["--op", name]].concat())?.op(),
            op
        );
    }
    assert_eq!(ClientOp::of_name("scrub"), None);
    let reading: Vec<ClientOp> = ClientOp::ALL
        .into_iter()
        .filter(|op| op.reads_only())
        .collect();
    assert_eq!(reading, [ClientOp::Read, ClientOp::Scan, ClientOp::Span]);
    Ok(())
}

#[test]
fn test_what_cannot_run_is_refused_by_name() {
    for (flag, value, named) in [
        ("--op", "scrub", "list-read or hot, not scrub"),
        ("--session", "maybe", "--session is on or off"),
        ("--fresh", "yes", "--fresh is on or off"),
        ("--level", "latest", "--level is snapshot or monotonic"),
        ("--limit", "4294967296", "--limit takes at most 4294967295"),
        ("--predicates", "0", "--predicates must be at least 1"),
        ("--predicates", "1025", "--predicates takes at most 1024"),
        ("--writes", "255", "--writes takes at most 254"),
        (
            "--value-bytes",
            "65537",
            "--value-bytes takes at most 65536",
        ),
        ("--address", "localhost", "a host and a port"),
        ("--connections", "0", "--connections must be at least 1"),
        ("--connections", "1025", "--connections takes at most 1024"),
        ("--relations", "1001", "--relations takes at most 1000"),
        ("--partitions", "1025", "--partitions takes at most 1024"),
        ("--seconds", "0", "--seconds must be at least 1"),
        ("--unknown", "1", "unknown flag --unknown"),
    ] {
        let mut args = REQUIRED.to_vec();
        args.extend([flag, value]);
        assert!(refused_naming(&args, named), "{flag} {value}");
    }
    // Fewer hubs than partitions leave a partition with none to point at.
    let mut few = REQUIRED.to_vec();
    few.extend(["--partitions", "4", "--hubs", "3"]);
    assert!(refused_naming(&few, "--hubs must be at least 4"));
    assert!(parsed(&[]).is_err(), "no arguments name no run");
    for missing in ["--address", "--password-file", "--op"] {
        let without: Vec<&str> = REQUIRED
            .chunks(2)
            .filter(|pair| pair.first() != Some(&missing))
            .flatten()
            .copied()
            .collect();
        assert!(
            refused_naming(&without, &format!("{missing} is required")),
            "{missing}"
        );
    }
}

/// A host's name and a port are resolved at start, as a bench Job dials a Service by its name.
#[test]
fn test_an_address_by_name_is_resolved() -> TestResult {
    let args: Vec<&str> = REQUIRED
        .iter()
        .map(|flag| {
            if *flag == "127.0.0.1:7080" {
                "localhost:7080"
            } else {
                *flag
            }
        })
        .collect();
    let parsed = parsed(&args)?;
    assert!(parsed.address().ip().is_loopback(), "{}", parsed.address());
    assert_eq!(parsed.address().port(), 7080);
    Ok(())
}
