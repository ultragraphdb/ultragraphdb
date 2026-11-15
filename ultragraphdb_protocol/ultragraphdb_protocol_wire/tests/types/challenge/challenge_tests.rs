/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyBuilder, Challenge};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_roles::{golden_role, role_bytes};

/// RFC 7677's salt, `W22ZaJ0SNY7soEsUEjb6gQ==` decoded.
pub const SALT: [u8; 16] = [
    0x5b, 0x6d, 0x99, 0x68, 0x9d, 0x12, 0x35, 0x8e, 0xec, 0xa0, 0x4b, 0x14, 0x12, 0x36, 0xfa, 0x81,
];

#[test]
fn test_a_challenge_builds_the_golden_request_and_reply() -> TestResult {
    let request = Challenge::request("user");
    assert_eq!((request.name(), request.iterations()), ("user", 0));
    assert!(request.salt().is_empty());
    let golden = golden_role("challenge")?;
    assert_eq!(request.frame(golden.header())?, role_bytes(&golden)?);
    let reply = Challenge::request("user").with_salt(4_096, SALT.to_vec());
    assert_eq!((reply.iterations(), reply.salt()), (4_096, &SALT[..]));
    let golden_2 = golden_role("challenge_reply")?;
    assert_eq!(reply.frame(golden_2.header())?, role_bytes(&golden_2)?);
    Ok(())
}

fn refused(challenge: &Challenge) -> TestResult<ProtocolErrorEnum> {
    let golden = golden_role("challenge")?;
    Ok(challenge
        .frame(golden.header())
        .err()
        .context("a refusal")?
        .0)
}

/// C1.78, C1.81, C1.82 at the builder: it refuses what its view refuses, and builds no frame: a count outside 4,096
/// to 10,000,000, a reply with no salt, and a request with one.
#[test]
fn test_the_builder_refuses_what_the_view_refuses() -> TestResult {
    for iterations in [1u32, 4_095, 10_000_001, u32::MAX] {
        let reply = Challenge::request("user").with_salt(iterations, SALT.to_vec());
        assert_eq!(
            refused(&reply)?,
            ProtocolErrorEnum::FieldValue {
                command: 519,
                field: "iterations",
                value: iterations.into()
            }
        );
    }
    let unsalted = Challenge::request("user").with_salt(4_096, Vec::new());
    assert_eq!(
        refused(&unsalted)?,
        ProtocolErrorEnum::CountMismatch {
            command: 519,
            field: "salt",
            count: 0,
            expected: 1
        }
    );
    let salted_request = Challenge::request("user").with_salt(0, vec![7]);
    assert_eq!(
        refused(&salted_request)?,
        ProtocolErrorEnum::CountMismatch {
            command: 519,
            field: "salt",
            count: 1,
            expected: 0
        }
    );
    Ok(())
}

/// C1.79, C1.84 at the builder: both ends of the count's range and a name of no byte are built.
#[test]
fn test_the_builder_builds_the_ends_of_the_range() -> TestResult {
    let golden = golden_role("challenge")?;
    for iterations in [4_096u32, 10_000_000] {
        let reply = Challenge::request("").with_salt(iterations, vec![1]);
        assert!(reply.frame(golden.header()).is_ok(), "{iterations}");
    }
    assert!(Challenge::request("").frame(golden.header()).is_ok());
    Ok(())
}
