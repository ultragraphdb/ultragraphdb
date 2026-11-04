/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_config::ConfigRecord;
use ultragraphdb_tool_bdd::{Context, TestResult};

/// Every kind reads back as it was written, the files untouched.
#[test]
fn test_a_record_round_trips() -> TestResult {
    let files: &[u8] = b"frontend 1\nx\n";
    for record in [
        ConfigRecord::Revision {
            target: "ci",
            number: 3,
            activate: false,
            files,
        },
        ConfigRecord::Revision {
            target: "ci",
            number: 1,
            activate: true,
            files,
        },
        ConfigRecord::Activate {
            target: "prod",
            number: 2,
        },
        ConfigRecord::Retire { target: "old" },
    ] {
        assert_eq!(ConfigRecord::decode(&record.encode())?, record);
    }
    Ok(())
}

/// What no writer writes is refused, each for its reason.
#[test]
fn test_a_malformed_record_is_refused() -> TestResult {
    let number = 1_u64.to_le_bytes();
    let retire_named = [&[4, 1, 0, b'c'][..], &number].concat();
    let activate_zero = [&[3, 1, 0, b'c'][..], &0_u64.to_le_bytes()].concat();
    let trailing = [&[3, 1, 0, b'c'][..], &number, b"x"].concat();
    let not_unicode = [&[3, 1, 0, 0xFF][..], &number].concat();
    let unknown = [&[9, 1, 0, b'c'][..], &number].concat();
    for (body, says) in [
        (&b""[..], "ends before"),
        (&[1][..], "ends before"),
        (&[1, 5, 0, b'c'][..], "ends before"),
        (&[1, 1, 0, b'c', 1][..], "ends before"),
        (&retire_named, "names a revision"),
        (&activate_zero, "is 0"),
        (&trailing, "bytes follow"),
        (&not_unicode, "not Unicode"),
        (&unknown, "kind 9"),
    ] {
        let refused = ConfigRecord::decode(body).err().context("a refusal")?;
        assert!(refused.to_string().contains(says), "{refused} says {says}");
    }
    Ok(())
}
