/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_config_check::TargetFiles;
use ultragraphdb_tool_bdd::{Context, TestResult};

/// A bundle reads back to the files it was made of, byte for byte, line breaks in them and an empty file included.
#[test]
fn test_a_bundle_round_trips() -> TestResult {
    let mut target = TargetFiles::new("ci");
    target.insert("frontend", "a.toml", b"x = 1\n\ny = 2\n".to_vec());
    target.insert("storage", "b.toml", Vec::new());
    let read = TargetFiles::from_bundle("ci", &target.to_bundle())?;
    assert_eq!(read.bytes("frontend"), target.bytes("frontend"));
    assert_eq!(read.bytes("storage"), Some(&b""[..]));
    assert_eq!(read.name(), "ci");
    assert_eq!(read.to_bundle(), target.to_bundle());
    assert_eq!(TargetFiles::from_bundle("ci", b"")?.files().count(), 0);
    Ok(())
}

/// Anything else is refused, saying where it stops being a bundle.
#[test]
fn test_a_malformed_bundle_is_refused() -> TestResult {
    for wrong in [
        &b"no length line"[..],
        b"frontend\n",
        b"frontend x\n",
        b"frontend 5\nab\n",
        b"frontend 1\nabc\n",
        b" 0\n\n",
        b"frontend 99999999999999999999999\n",
    ] {
        let refused = TargetFiles::from_bundle("ci", wrong)
            .err()
            .context("a refusal")?;
        assert!(
            refused.to_string().contains("malformed at byte"),
            "{refused}"
        );
    }
    Ok(())
}
