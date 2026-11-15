/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Challenge (`PROTOCOL.md` §13.8): `iterations` u32 at 0, section 0 the principal's name, section 1 the salt. Order:
//! unused bytes 4–127 zero; `iterations`; sections; section 0 UTF-8; section 1 empty in a request and not empty in a
//! reply.

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{Challenge, ChallengeView, Command};

use crate::utils::layout::{
    assert_builds_as_laid_out, built, command_header, count, field, frame, nonzero_byte, outcome,
    read_back, reserved, sections, viewed, within,
};

/// The Challenge frame of `iterations`, `name` and `salt` laid out by hand, `unused` a command-header byte set outside
/// the field.
fn laid_out(
    iterations: u32,
    unused: Option<(usize, u8)>,
    name: &[u8],
    salt: &[u8],
) -> Option<Vec<u8>> {
    let mut header = command_header(&[(0, &iterations.to_le_bytes())]);
    if let Some((at, byte)) = unused {
        header[at] = byte;
    }
    let body = sections(&[name.to_vec(), salt.to_vec()]);
    frame(Command::Challenge, header, &body, 0)
}

/// A drawn count a reply names, 4,096 to 10,000,000.
fn count_in_range(tc: &TestCase) -> u32 {
    tc.draw(gs::integers::<u32>().min_value(4_096).max_value(10_000_000))
}

/// A drawn count no body names: 1 to 4,095, or above 10,000,000.
fn count_out_of_range(tc: &TestCase) -> u32 {
    if tc.draw(gs::booleans()) {
        tc.draw(gs::integers::<u32>().min_value(1).max_value(4_095))
    } else {
        tc.draw(gs::integers::<u32>().min_value(10_000_001))
    }
}

fn salt(tc: &TestCase) -> Vec<u8> {
    tc.draw(gs::vecs(gs::integers::<u8>()).min_size(1).max_size(64))
}

/// L1.9: a Challenge request, and a reply of a drawn count and a salt of 1 to 64 bytes, is the bytes laid out by hand,
/// reads back to its fields and builds again to the same bytes.
#[hegel::test]
fn test_a_challenge_round_trips(tc: TestCase) {
    let name: String = tc.draw(gs::text().max_size(24));
    let (iterations, salt) = if tc.draw(gs::booleans()) {
        (count_in_range(&tc), salt(&tc))
    } else {
        (0, Vec::new())
    };
    let expected = laid_out(iterations, None, name.as_bytes(), &salt);
    let request = Challenge::request(name.clone());
    let body = if iterations == 0 {
        request
    } else {
        request.with_salt(iterations, salt.clone())
    };
    assert!(expected.is_some());
    assert_eq!(built(&body).ok(), expected);
    let bytes = expected.unwrap_or_default();
    assert_eq!(
        viewed::<ChallengeView, _>(&bytes, |view| {
            (
                view.name().to_string(),
                view.iterations(),
                view.salt().to_vec(),
            )
        }),
        Ok((name, iterations, salt))
    );
    let again = read_back::<ChallengeView>(&bytes);
    assert_eq!(again, Some(body));
    assert_eq!(again.and_then(|body| built(&body).ok()), Some(bytes));
}

/// The rules of a Challenge reply's Order line broken where `broken` says, and each rule's refusal: an unused byte, a
/// count out of range, a name that is not UTF-8, and no salt.
fn reply(
    tc: &TestCase,
    broken: impl Fn(usize) -> bool,
) -> (Option<Vec<u8>>, [ProtocolErrorEnum; 4]) {
    let at = within(tc, 4, 127);
    let out_of_range = count_out_of_range(tc);
    let mut name: Vec<u8> = tc.draw(gs::text().max_size(24)).into_bytes();
    let valid = name.len();
    if broken(2) {
        name.push(0xFF);
    }
    let iterations = if broken(1) {
        out_of_range
    } else {
        count_in_range(tc)
    };
    let salt = if broken(3) { Vec::new() } else { salt(tc) };
    let unused = broken(0).then(|| (at, nonzero_byte(tc)));
    let refusals = [
        reserved(at),
        field(519, "iterations", out_of_range.into()),
        ProtocolErrorEnum::NotUtf8 {
            command: 519,
            section: 0,
            offset: valid,
        },
        count(519, "salt", 0, 1),
    ];
    (laid_out(iterations, unused, &name, &salt), refusals)
}

/// L1.19: a Challenge with one drawn rule broken is refused with that rule's refusal; a request holding a salt too.
#[hegel::test]
fn test_a_challenge_breaking_one_rule_is_refused_with_that_rule(tc: TestCase) {
    let rule = within(&tc, 0, 4);
    if rule == 4 {
        let salt = salt(&tc);
        let laid = laid_out(0, None, b"user", &salt);
        assert_eq!(
            outcome::<ChallengeView>(&laid.unwrap_or_default()),
            Err(count(519, "salt", salt.len() as u64, 0))
        );
    } else {
        let (laid, refusals) = reply(&tc, |broken| broken == rule);
        assert!(laid.is_some());
        assert_eq!(
            outcome::<ChallengeView>(&laid.unwrap_or_default()),
            Err(refusals[rule].clone())
        );
    }
}

/// L1.22: with two rules of the Order line broken, the refusal is that of the earlier one.
#[hegel::test]
fn test_a_challenge_breaking_two_rules_is_refused_with_the_earlier(tc: TestCase) {
    let first = within(&tc, 0, 2);
    let second = within(&tc, first + 1, 3);
    let (laid, refusals) = reply(&tc, |broken| broken == first || broken == second);
    assert!(laid.is_some());
    assert_eq!(
        outcome::<ChallengeView>(&laid.unwrap_or_default()),
        Err(refusals[first].clone())
    );
}

/// L1.23: the builder refuses exactly when the view refuses the same fields laid out by hand, with the same refusal:
/// a count out of range, a reply with no salt, and a request with one.
#[hegel::test]
fn test_a_challenge_builder_refuses_what_its_view_refuses(tc: TestCase) {
    let name: String = tc.draw(gs::text().max_size(24));
    let iterations = match within(&tc, 0, 2) {
        0 => 0,
        1 => count_in_range(&tc),
        _ => count_out_of_range(&tc),
    };
    let salt = if tc.draw(gs::booleans()) {
        salt(&tc)
    } else {
        Vec::new()
    };
    let expected = laid_out(iterations, None, name.as_bytes(), &salt);
    let body = Challenge::request(name).with_salt(iterations, salt.clone());
    assert_builds_as_laid_out::<_, ChallengeView>(&body, &expected);
    let valid = (iterations == 0 && salt.is_empty())
        || ((4_096..=10_000_000).contains(&iterations) && !salt.is_empty());
    assert_eq!(built(&body).is_ok(), valid);
}
