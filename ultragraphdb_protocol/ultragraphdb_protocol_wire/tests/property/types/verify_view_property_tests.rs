/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Verify (`PROTOCOL.md` §13.9): `verified` u8 at 0, `principal` u64 at 8, and four sections: the name, the
//! AuthMessage, the ClientProof and the ServerSignature. Order: unused bytes 1–7 and 16–127 zero; `verified`;
//! `principal`; sections; sections 0 and 1 UTF-8; section 2 of 32 bytes; section 3 of 32 bytes in a verified reply,
//! else empty.

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{Command, Verify, VerifyView};

use crate::utils::layout::{
    assert_builds_as_laid_out, built, command_header, count, field, frame, nonzero_byte, outcome,
    read_back, reserved, sections, viewed, within,
};

/// The Verify frame laid out by hand, `unused` a command-header byte set outside the fields.
fn laid_out(
    verified: u8,
    principal: u64,
    unused: Option<(usize, u8)>,
    parts: [&[u8]; 4],
) -> Option<Vec<u8>> {
    let mut header = command_header(&[(0, &[verified]), (8, &principal.to_le_bytes())]);
    if let Some((at, byte)) = unused {
        header[at] = byte;
    }
    let body = sections(&parts.map(<[u8]>::to_vec));
    frame(Command::Verify, header, &body, 0)
}

fn proof(tc: &TestCase) -> [u8; 32] {
    tc.draw(gs::arrays(gs::integers::<u8>()))
}

/// An unused command-header offset: 1 to 7, or 16 to 127.
fn unused_at(tc: &TestCase) -> usize {
    tc.draw(gs::sampled_from(
        (1usize..=7).chain(16..=127).collect::<Vec<usize>>(),
    ))
}

/// L1.10: a Verify request, the reply of a proof that does not hold, which is the same body, and a verified reply
/// naming a drawn principal and signature, is the bytes laid out by hand, reads back to its fields and builds again
/// to the same bytes.
#[hegel::test]
fn test_a_verify_round_trips(tc: TestCase) {
    let name: String = tc.draw(gs::text().max_size(24));
    let auth_message: String = tc.draw(gs::text().max_size(64));
    let client_proof = proof(&tc);
    let verified = tc
        .draw(gs::booleans())
        .then(|| (tc.draw(gs::integers::<u64>().min_value(1)), proof(&tc)));
    let (principal, signature) = verified.map_or((0, Vec::new()), |(p, s)| (p, s.to_vec()));
    let expected = laid_out(
        u8::from(verified.is_some()),
        principal,
        None,
        [
            name.as_bytes(),
            auth_message.as_bytes(),
            &client_proof,
            &signature,
        ],
    );
    let request = Verify::request(name.clone(), auth_message.clone(), client_proof);
    let body = match verified {
        Some((verifier, signed)) => request.with_verified(verifier, signed),
        None => request,
    };
    assert!(expected.is_some());
    assert_eq!(built(&body).ok(), expected);
    let bytes = expected.unwrap_or_default();
    assert_eq!(
        viewed::<VerifyView, _>(&bytes, |view| {
            (
                view.name().to_string(),
                view.auth_message().to_string(),
                *view.proof(),
                view.verified(),
                view.principal(),
                view.signature().copied(),
            )
        }),
        Ok((
            name,
            auth_message,
            client_proof,
            verified.is_some(),
            principal,
            verified.map(|(_, signed)| signed)
        ))
    );
    let again = read_back::<VerifyView>(&bytes);
    assert_eq!(again, Some(body));
    assert_eq!(again.and_then(|rebuilt| built(&rebuilt).ok()), Some(bytes));
}

/// The rules of a verified reply's Order line broken where `broken` says, and each rule's refusal: an unused byte,
/// `verified` above 1, `principal` 0, a name and an AuthMessage that are not UTF-8, a proof and a signature not of 32
/// bytes.
fn reply(
    tc: &TestCase,
    broken: impl Fn(usize) -> bool,
) -> (Option<Vec<u8>>, [ProtocolErrorEnum; 7]) {
    let at = unused_at(tc);
    let verified = tc.draw(gs::integers::<u8>().min_value(2));
    let mut name: Vec<u8> = tc.draw(gs::text().max_size(24)).into_bytes();
    let mut auth_message: Vec<u8> = tc.draw(gs::text().max_size(64)).into_bytes();
    let (name_len, auth_len) = (name.len(), auth_message.len());
    let proof_len = tc.draw(gs::sampled_from(vec![0usize, 1, 31, 33, 64]));
    let signature_len = tc.draw(gs::sampled_from(vec![0usize, 1, 31, 33, 64]));
    if broken(3) {
        name.push(0xFF);
    }
    if broken(4) {
        auth_message.push(0xFF);
    }
    let client_proof = vec![0xA5; if broken(5) { proof_len } else { 32 }];
    let signature = vec![0x5A; if broken(6) { signature_len } else { 32 }];
    let principal = if broken(2) {
        0
    } else {
        tc.draw(gs::integers::<u64>().min_value(1))
    };
    let unused = broken(0).then(|| (at, nonzero_byte(tc)));
    let refusals = [
        reserved(at),
        field(520, "verified", verified.into()),
        field(520, "principal", 0),
        ProtocolErrorEnum::NotUtf8 {
            command: 520,
            section: 0,
            offset: name_len,
        },
        ProtocolErrorEnum::NotUtf8 {
            command: 520,
            section: 1,
            offset: auth_len,
        },
        count(520, "proof", proof_len as u64, 32),
        count(520, "signature", signature_len as u64, 32),
    ];
    let laid = laid_out(
        if broken(1) { verified } else { 1 },
        principal,
        unused,
        [&name, &auth_message, &client_proof, &signature],
    );
    (laid, refusals)
}

/// L1.20: a Verify with one drawn rule broken is refused with that rule's refusal; an unverified body naming a
/// principal, or holding a signature, too.
#[hegel::test]
fn test_a_verify_breaking_one_rule_is_refused_with_that_rule(tc: TestCase) {
    let rule = within(&tc, 0, 8);
    if rule == 7 {
        let principal = tc.draw(gs::integers::<u64>().min_value(1));
        let laid = laid_out(0, principal, None, [b"user", b"m", &[7; 32], &[]]);
        assert_eq!(
            outcome::<VerifyView>(&laid.unwrap_or_default()),
            Err(field(520, "principal", principal))
        );
    } else if rule == 8 {
        let signature = vec![9u8; within(&tc, 1, 64)];
        let laid = laid_out(0, 0, None, [b"user", b"m", &[7; 32], &signature]);
        assert_eq!(
            outcome::<VerifyView>(&laid.unwrap_or_default()),
            Err(count(520, "signature", signature.len() as u64, 0))
        );
    } else {
        let (laid, refusals) = reply(&tc, |broken| broken == rule);
        assert!(laid.is_some());
        assert_eq!(
            outcome::<VerifyView>(&laid.unwrap_or_default()),
            Err(refusals[rule].clone())
        );
    }
}

/// L1.22: with two rules of the Order line broken, the refusal is that of the earlier one.
#[hegel::test]
fn test_a_verify_breaking_two_rules_is_refused_with_the_earlier(tc: TestCase) {
    let first = within(&tc, 0, 5);
    let second = within(&tc, first + 1, 6);
    let (laid, refusals) = reply(&tc, |broken| broken == first || broken == second);
    assert!(laid.is_some());
    assert_eq!(
        outcome::<VerifyView>(&laid.unwrap_or_default()),
        Err(refusals[first].clone())
    );
}

/// L1.23: the builder refuses exactly when the view refuses the same fields laid out by hand, with the same refusal:
/// a verified reply of principal 0.
#[hegel::test]
fn test_a_verify_builder_refuses_what_its_view_refuses(tc: TestCase) {
    let name: String = tc.draw(gs::text().max_size(24));
    let auth_message: String = tc.draw(gs::text().max_size(64));
    let client_proof = proof(&tc);
    let signature = proof(&tc);
    let principal = tc.draw(gs::sampled_from(vec![0u64, 0, 1, 7, u64::MAX]));
    let expected = laid_out(
        1,
        principal,
        None,
        [
            name.as_bytes(),
            auth_message.as_bytes(),
            &client_proof,
            &signature,
        ],
    );
    let body =
        Verify::request(name, auth_message, client_proof).with_verified(principal, signature);
    assert_builds_as_laid_out::<_, VerifyView>(&body, &expected);
    assert_eq!(built(&body).is_err(), principal == 0);
}
