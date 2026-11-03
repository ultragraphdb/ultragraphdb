/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{BodyBuilder, Verify};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_roles::{golden_role, role_bytes};

/// RFC 7677's AuthMessage.
pub const AUTH_MESSAGE: &str = "n=user,r=rOprNGfwEbeRWgbNEkqO,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0,s=W22ZaJ0SNY7soEsUEjb6gQ==,i=4096,c=biws,r=rOprNGfwEbeRWgbNEkqO%hvYDpWUa2RaTCAfuxFIlj)hNlF$k0";

/// RFC 7677's ClientProof, `dHzbZapWIk4jUhN+Ute9ytag9zjfMHgsqmmiz7AndVQ=` decoded.
pub const PROOF: [u8; 32] = [
    0x74, 0x7c, 0xdb, 0x65, 0xaa, 0x56, 0x22, 0x4e, 0x23, 0x52, 0x13, 0x7e, 0x52, 0xd7, 0xbd, 0xca,
    0xd6, 0xa0, 0xf7, 0x38, 0xdf, 0x30, 0x78, 0x2c, 0xaa, 0x69, 0xa2, 0xcf, 0xb0, 0x27, 0x75, 0x54,
];

/// RFC 7677's ServerSignature, `6rriTRBi23WpRR/wtup+mMhUZUn/dB5nLTJRsjl95G4=` decoded.
pub const SIGNATURE: [u8; 32] = [
    0xea, 0xba, 0xe2, 0x4d, 0x10, 0x62, 0xdb, 0x75, 0xa9, 0x45, 0x1f, 0xf0, 0xb6, 0xea, 0x7e, 0x98,
    0xc8, 0x54, 0x65, 0x49, 0xff, 0x74, 0x1e, 0x67, 0x2d, 0x32, 0x51, 0xb2, 0x39, 0x7d, 0xe4, 0x6e,
];

#[test]
fn test_a_verify_builds_the_golden_request_and_reply() -> TestResult {
    let request = Verify::request("user", AUTH_MESSAGE, PROOF);
    assert_eq!(
        (request.name(), request.auth_message()),
        ("user", AUTH_MESSAGE)
    );
    assert_eq!(request.proof(), &PROOF);
    assert_eq!(
        (request.verified(), request.principal(), request.signature()),
        (false, 0, None)
    );
    let golden = golden_role("verify")?;
    assert_eq!(request.frame(golden.header())?, role_bytes(&golden)?);
    let reply = request.with_verified(7, SIGNATURE);
    assert_eq!(
        (reply.verified(), reply.principal(), reply.signature()),
        (true, 7, Some(&SIGNATURE))
    );
    let golden_2 = golden_role("verify_reply")?;
    assert_eq!(reply.frame(golden_2.header())?, role_bytes(&golden_2)?);
    Ok(())
}
