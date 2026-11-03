/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Challenge};
use ultragraphdb_tool_bdd::TestResult;

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
