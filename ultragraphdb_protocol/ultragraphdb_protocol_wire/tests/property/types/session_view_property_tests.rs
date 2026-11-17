/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Session (`PROTOCOL.md` §13.23): `session` 16 bytes at 0, `principal` u64 at 16, no sections. Order: unused bytes
//! 24–127 zero; `session`; sections.

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{Command, Session, SessionView};

use crate::utils::layout::{
    assert_builds_as_laid_out, built, command_header, field, frame, nonzero_byte, outcome,
    read_back, reserved, sections, session, viewed, within,
};

/// The frame of a Session laid out by hand, `unused` a command-header byte set outside the fields, `body` its body.
fn laid_out(
    session: [u8; 16],
    principal: u64,
    unused: Option<(usize, u8)>,
    body: &[u8],
) -> Option<Vec<u8>> {
    let mut fields = command_header(&[(0, &session), (16, &principal.to_le_bytes())]);
    if let Some((at, byte)) = unused {
        fields[at] = byte;
    }
    frame(Command::Session, fields, body, 0)
}

/// L1.1: a Session built from the drawn fields is the bytes laid out by hand, reads back to the fields, and builds
/// again to the same bytes; the reply is the same body.
#[hegel::test]
fn test_a_session_round_trips(tc: TestCase) {
    let (session, principal) = (session(&tc), tc.draw(gs::integers::<u64>()));
    let expected = laid_out(session, principal, None, &sections(&[]));
    let body = Session::new(session, principal);
    assert!(expected.is_some());
    assert_eq!(built(&body).ok(), expected);
    let bytes = expected.unwrap_or_default();
    assert_eq!(
        viewed::<SessionView, _>(&bytes, |view| (view.session(), view.principal())),
        Ok((session, principal))
    );
    let again = read_back::<SessionView>(&bytes);
    assert_eq!(again, Some(body));
    assert_eq!(again.and_then(|rebuilt| built(&rebuilt).ok()), Some(bytes));
}

/// One rule of a Session's Order line: the unused byte it sets, whether it zeroes the session, whether it gives the
/// frame a body, and the refusal it gives.
type Rule = (Option<(usize, u8)>, bool, bool, ProtocolErrorEnum);

/// The rules of a Session's Order line, in order, each with the refusal it gives the drawn fields.
fn rules(tc: &TestCase) -> [Rule; 3] {
    let at = within(tc, 24, 127);
    [
        (Some((at, nonzero_byte(tc))), false, false, reserved(at)),
        (None, true, false, field(534, "session", 0)),
        (
            None,
            false,
            true,
            ProtocolErrorEnum::TrailingBytes {
                command: 534,
                end: 0,
                len: 8,
            },
        ),
    ]
}

/// L1.14: a Session with one drawn rule broken is refused with that rule's refusal.
#[hegel::test]
fn test_a_session_breaking_one_rule_is_refused_with_that_rule(tc: TestCase) {
    let (drawn, principal) = (session(&tc), tc.draw(gs::integers::<u64>()));
    let rules = rules(&tc);
    let (unused, zero, trailing, refusal) = &rules[within(&tc, 0, 2)];
    let body = if *trailing { vec![0; 8] } else { sections(&[]) };
    let session = if *zero { [0; 16] } else { drawn };
    let bytes = laid_out(session, principal, *unused, &body);
    assert!(bytes.is_some());
    assert_eq!(
        outcome::<SessionView>(&bytes.unwrap_or_default()),
        Err(refusal.clone())
    );
}

/// L1.22: with two rules of the Order line broken, the refusal is that of the earlier one.
#[hegel::test]
fn test_a_session_breaking_two_rules_is_refused_with_the_earlier(tc: TestCase) {
    let (drawn, principal) = (session(&tc), tc.draw(gs::integers::<u64>()));
    let rules = rules(&tc);
    let first = within(&tc, 0, 1);
    let second = within(&tc, first + 1, 2);
    let broken = |index: usize| index == first || index == second;
    let unused = rules[0].0.filter(|_| broken(0));
    let session = if broken(1) { [0; 16] } else { drawn };
    let body = if broken(2) { vec![0; 8] } else { sections(&[]) };
    let bytes = laid_out(session, principal, unused, &body);
    assert!(bytes.is_some());
    assert_eq!(
        outcome::<SessionView>(&bytes.unwrap_or_default()),
        Err(rules[first].3.clone())
    );
}

/// L1.23: the builder refuses exactly when the view refuses the same fields laid out by hand, with the same refusal.
#[hegel::test]
fn test_a_session_builder_refuses_what_its_view_refuses(tc: TestCase) {
    let zero = tc.draw(gs::booleans());
    let session = if zero { [0; 16] } else { session(&tc) };
    let principal = tc.draw(gs::integers::<u64>());
    let expected = laid_out(session, principal, None, &sections(&[]));
    assert_builds_as_laid_out::<_, SessionView>(&Session::new(session, principal), &expected);
    assert_eq!(built(&Session::new(session, principal)).is_err(), zero);
}
