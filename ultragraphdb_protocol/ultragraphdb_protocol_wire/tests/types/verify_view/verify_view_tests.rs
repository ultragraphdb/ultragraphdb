/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, Verify, VerifyView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::verify::verify_tests::{AUTH_MESSAGE, PROOF, SIGNATURE};
use crate::utils::golden_bodies::{frame_of, put, view_any, with_command_header, with_section};
use crate::utils::golden_roles::{golden_role, role_bytes};

fn bytes(name: &str) -> TestResult<Vec<u8>> {
    role_bytes(&golden_role(name)?)
}

fn refused(bytes: &[u8]) -> TestResult<ProtocolErrorEnum> {
    Ok(VerifyView::from_frame(&frame_of(bytes)?)
        .err()
        .context("a refusal")?
        .0)
}

fn count(field: &'static str, count: u64, expected: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::CountMismatch {
        command: 520,
        field,
        count,
        expected,
    }
}

#[test]
fn test_the_golden_verifies_read_back_to_their_bodies() -> TestResult {
    let request = bytes("verify")?;
    let view = VerifyView::from_frame(&frame_of(&request)?)?;
    assert_eq!(
        (view.name(), view.auth_message(), view.proof()),
        ("user", AUTH_MESSAGE, &PROOF)
    );
    assert_eq!(
        (view.verified(), view.principal(), view.signature()),
        (false, 0, None)
    );
    assert_eq!(
        view.to_body()?,
        Verify::request("user", AUTH_MESSAGE, PROOF)
    );
    let reply = bytes("verify_reply")?;
    let view_2 = VerifyView::from_frame(&frame_of(&reply)?)?;
    assert_eq!(
        (view_2.verified(), view_2.principal(), view_2.signature()),
        (true, 7, Some(&SIGNATURE))
    );
    assert_eq!(
        view_2.to_body()?,
        Verify::request("user", AUTH_MESSAGE, PROOF).with_verified(7, SIGNATURE)
    );
    for frame in [request, reply] {
        view_any(&frame)??;
    }
    Ok(())
}

#[test]
fn test_the_header_is_checked_unused_bytes_verified_then_principal() -> TestResult {
    let reply = bytes("verify_reply")?;
    for at in [1usize, 7, 16, 127] {
        let bad = with_command_header(&reply, |h| {
            h[at] = 1;
            h[0] = 2;
        });
        assert_eq!(
            refused(&bad)?,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    let bad_2 = with_command_header(&reply, |h| h[0] = 2);
    assert_eq!(
        refused(&bad_2)?,
        ProtocolErrorEnum::FieldValue {
            command: 520,
            field: "verified",
            value: 2
        }
    );
    let bad_3 = with_command_header(&bytes("verify")?, |h| put(h, 8, 7, 8));
    assert_eq!(
        refused(&bad_3)?,
        ProtocolErrorEnum::FieldValue {
            command: 520,
            field: "principal",
            value: 7
        }
    );
    Ok(())
}

#[test]
fn test_names_are_utf8_before_the_proof_is_counted() -> TestResult {
    let reply = bytes("verify_reply")?;
    let bad = with_section(&reply, 4, 0, |s| s[0] = 0xC0);
    let bad_2 = with_section(&bad, 4, 2, |s: &mut Vec<u8>| {
        s.truncate(s.len().saturating_sub(1))
    });
    assert_eq!(
        refused(&bad_2)?,
        ProtocolErrorEnum::NotUtf8 {
            command: 520,
            section: 0,
            offset: 0
        }
    );
    let bad_3 = with_section(&reply, 4, 1, |s| s[3] = 0xFF);
    let bad_4 = with_section(&bad_3, 4, 2, |s: &mut Vec<u8>| {
        s.truncate(s.len().saturating_sub(1))
    });
    assert_eq!(
        refused(&bad_4)?,
        ProtocolErrorEnum::NotUtf8 {
            command: 520,
            section: 1,
            offset: 3
        }
    );
    Ok(())
}

#[test]
fn test_the_proof_and_the_signature_are_32_bytes_where_present() -> TestResult {
    let reply = bytes("verify_reply")?;
    assert_eq!(
        refused(&with_section(&reply, 4, 2, |s: &mut Vec<u8>| s
            .truncate(s.len().saturating_sub(1))))?,
        count("proof", 31, 32)
    );
    assert_eq!(
        refused(&with_section(&reply, 4, 2, |s| s.push(0)))?,
        count("proof", 33, 32)
    );
    assert_eq!(
        refused(&with_section(&reply, 4, 3, |s: &mut Vec<u8>| s
            .truncate(s.len().saturating_sub(1))))?,
        count("signature", 31, 32)
    );
    assert_eq!(
        refused(&with_section(&reply, 4, 3, Vec::clear))?,
        count("signature", 0, 32)
    );
    // A request, or a proof that does not hold, carries no signature.
    let signed = with_section(&bytes("verify")?, 4, 3, |s| s.extend_from_slice(&SIGNATURE));
    assert_eq!(refused(&signed)?, count("signature", 32, 0));
    Ok(())
}
