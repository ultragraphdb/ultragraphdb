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

/// C1.87: `verified` is a boolean.
#[test]
fn test_verified_is_a_boolean() -> TestResult {
    for value in [2u8, 255] {
        let bad = with_command_header(&bytes("verify")?, |h| h[0] = value);
        assert_eq!(
            refused(&bad)?,
            ProtocolErrorEnum::FieldValue {
                command: 520,
                field: "verified",
                value: value.into()
            }
        );
    }
    Ok(())
}

/// C1.89: a verified reply names its principal, at least 1 (`PROTOCOL.md` §13.9).
#[test]
fn test_a_verified_reply_of_principal_zero_is_refused() -> TestResult {
    let bad = with_command_header(&bytes("verify_reply")?, |h| put(h, 8, 0, 8));
    assert_eq!(
        refused(&bad)?,
        ProtocolErrorEnum::FieldValue {
            command: 520,
            field: "principal",
            value: 0
        }
    );
    // The principal is checked before the sections: a verified reply of principal 0 whose name is not UTF-8.
    let bad_2 = with_section(&bad, 4, 0, |s| s[0] = 0xC0);
    assert_eq!(
        refused(&bad_2)?,
        ProtocolErrorEnum::FieldValue {
            command: 520,
            field: "principal",
            value: 0
        }
    );
    let least = with_command_header(&bytes("verify_reply")?, |h| put(h, 8, 1, 8));
    assert_eq!(VerifyView::from_frame(&frame_of(&least)?)?.principal(), 1);
    let most = with_command_header(&bytes("verify_reply")?, |h| put(h, 8, u64::MAX, 8));
    assert_eq!(
        VerifyView::from_frame(&frame_of(&most)?)?.principal(),
        u64::MAX
    );
    Ok(())
}

/// C1.90: with both names not UTF-8, section 0's refusal comes first.
#[test]
fn test_section_0_is_checked_before_section_1() -> TestResult {
    let bad = with_section(&bytes("verify")?, 4, 0, |s| s[2] = 0xFF);
    let bad_2 = with_section(&bad, 4, 1, |s| s[3] = 0xFF);
    assert_eq!(
        refused(&bad_2)?,
        ProtocolErrorEnum::NotUtf8 {
            command: 520,
            section: 0,
            offset: 2
        }
    );
    Ok(())
}

/// C1.91, C1.92: a proof of no byte, and a signature of 33 bytes in a verified reply.
#[test]
fn test_a_proof_of_no_byte_and_a_signature_of_33_are_refused() -> TestResult {
    let reply = bytes("verify_reply")?;
    assert_eq!(
        refused(&with_section(&reply, 4, 2, Vec::clear))?,
        count("proof", 0, 32)
    );
    assert_eq!(
        refused(&with_section(&reply, 4, 3, |s| s.push(0)))?,
        count("signature", 33, 32)
    );
    Ok(())
}

/// C1.94: the proof is counted before the signature.
#[test]
fn test_the_proof_is_counted_before_the_signature() -> TestResult {
    let cut = |s: &mut Vec<u8>| s.truncate(31);
    let reply = with_section(&with_section(&bytes("verify_reply")?, 4, 2, cut), 4, 3, cut);
    assert_eq!(refused(&reply)?, count("proof", 31, 32));
    Ok(())
}

/// C1.95: a request and the reply of a proof that does not hold are the same body, `verified` 0.
#[test]
fn test_a_request_and_an_unverified_reply_are_the_same_bytes() -> TestResult {
    let request = bytes("verify")?;
    let view = VerifyView::from_frame(&frame_of(&request)?)?;
    assert!(!view.verified());
    let reply = view.to_body()?;
    assert_eq!(reply, Verify::request("user", AUTH_MESSAGE, PROOF));
    assert_eq!(view_any(&request)?, Ok(()));
    Ok(())
}

/// The decoder checks section 0 as UTF-8 alone: a name of no byte reads.
#[test]
fn test_a_name_of_no_byte_reads() -> TestResult {
    let request = with_section(&bytes("verify")?, 4, 0, Vec::clear);
    assert_eq!(VerifyView::from_frame(&frame_of(&request)?)?.name(), "");
    Ok(())
}
