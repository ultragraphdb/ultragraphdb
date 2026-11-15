/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Check (`PROTOCOL.md` §13.24): a command header of all zero and one section of 32-byte checks, each `session` 16
//! bytes, `principal` u64, `kind` u8, `outcome` u8 and 6 zero bytes. Order: command header bytes zero; sections;
//! section 0 a whole number of checks, at least one; each check in order: bytes 26–31 zero, `session` not all zero and
//! above the one before, `kind` 1 or 2, `outcome` 0, 1, 12, 14 or 21.

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    Check, CheckKind, CheckOutcome, CheckView, Command, SessionCheck,
};

use crate::utils::layout::{
    assert_builds_as_laid_out, built, command_header, count, field, frame, nonzero_byte, outcome,
    read_back, reserved, section_length, sections, sessions, viewed, within,
};

/// A check as its four fields: `session`, `principal`, `kind` and `outcome`, the last two by their numbers.
type Fields = ([u8; 16], u64, u8, u8);

/// The body offset of section 0: the one-entry section table before it (§4.1).
const SECTION_0: usize = 8;

/// 1 to `max` drawn checks of ascending sessions, each with a drawn principal and kind; `answered` draws each outcome
/// from 1, 12, 14 and 21, a reply's, and leaves it 0 otherwise.
fn checks(tc: &TestCase, min: usize, max: usize, answered: bool) -> Vec<Fields> {
    sessions(tc, min, max)
        .into_iter()
        .map(|session| {
            let outcome = if answered {
                tc.draw(gs::sampled_from(vec![1u8, 12, 14, 21]))
            } else {
                0
            };
            (
                session,
                tc.draw(gs::integers::<u64>()),
                tc.draw(gs::sampled_from(vec![1u8, 2])),
                outcome,
            )
        })
        .collect()
}

/// The checks' section laid out by hand.
fn section(checks: &[Fields]) -> Vec<u8> {
    checks
        .iter()
        .flat_map(|(session, principal, kind, outcome)| {
            let mut record = session.to_vec();
            record.extend_from_slice(&principal.to_le_bytes());
            record.extend_from_slice(&[*kind, *outcome, 0, 0, 0, 0, 0, 0]);
            record
        })
        .collect()
}

/// The Check frame of `section` laid out by hand under the command header `fields`.
fn laid_out(fields: [u8; 128], section: Vec<u8>) -> Option<Vec<u8>> {
    frame(Command::Check, fields, &sections(&[section]), 0)
}

/// The kind numbered `number`, by the table of §13.24.
fn kind_of(number: u8) -> CheckKind {
    if number == 1 {
        CheckKind::Asks
    } else {
        CheckKind::Marks
    }
}

/// The outcome numbered `number`, by the table of §13.24.
fn outcome_of(number: u8) -> CheckOutcome {
    match number {
        1 => CheckOutcome::Passed,
        12 => CheckOutcome::NotHeld,
        14 => CheckOutcome::InFlight,
        21 => CheckOutcome::OtherPrincipal,
        _ => CheckOutcome::Unanswered,
    }
}

/// `fields` as the owned checks a builder takes.
fn owned(fields: &[Fields]) -> Vec<SessionCheck> {
    fields
        .iter()
        .map(|(session, principal, kind, outcome)| {
            SessionCheck::new(*session, *principal, kind_of(*kind))
                .with_outcome(outcome_of(*outcome))
        })
        .collect()
}

/// The checks a view reads from `bytes`, as fields.
fn read(bytes: &[u8]) -> Result<Vec<Fields>, ProtocolErrorEnum> {
    viewed::<CheckView, _>(bytes, |view| {
        (0..view.len())
            .filter_map(|index| view.get(index))
            .map(|check| {
                (
                    check.session(),
                    check.principal(),
                    check.kind().number(),
                    check.outcome().number(),
                )
            })
            .collect()
    })
}

fn round_trips(drawn: &[Fields]) {
    let expected = laid_out(command_header(&[]), section(drawn));
    let body = Check::new(owned(drawn));
    assert!(expected.is_some());
    assert_eq!(built(&body).ok(), expected);
    let bytes = expected.unwrap_or_default();
    assert_eq!(read(&bytes), Ok(drawn.to_vec()));
    let again = read_back::<CheckView>(&bytes);
    assert_eq!(again, Some(body));
    assert_eq!(again.and_then(|body| built(&body).ok()), Some(bytes));
}

/// L1.2: a Check request of 1 to 64 drawn checks is the bytes laid out by hand, reads back to its fields and builds
/// again to the same bytes.
#[hegel::test]
fn test_a_check_request_round_trips(tc: TestCase) {
    round_trips(&checks(&tc, 1, 64, false));
}

/// L1.3: the reply, each outcome drawn from 1, 12, 14 and 21, round-trips the same way.
#[hegel::test]
fn test_a_check_reply_round_trips(tc: TestCase) {
    round_trips(&checks(&tc, 1, 64, true));
}

/// One rule of a check, broken in check `index` of `drawn`; the refusal it gives.
fn break_check(
    tc: &TestCase,
    drawn: &[Fields],
    section: &mut [u8],
    index: usize,
    aspect: usize,
) -> ProtocolErrorEnum {
    let at = 32 * index;
    match aspect {
        0 => {
            let byte = within(tc, 26, 31);
            section[at + byte] = nonzero_byte(tc);
            ProtocolErrorEnum::PaddingNotZero {
                command: 535,
                offset: SECTION_0 + at + byte,
            }
        }
        1 => {
            // A session of all zero, one equal to the one before, or the one before's place taken by a later one.
            let previous = index.checked_sub(1).map(|before| drawn[before].0);
            let session = match (within(tc, 0, 2), previous) {
                (1, Some(before)) => before,
                (2, Some(before)) => {
                    let mut below = before;
                    below[15] = below[15].wrapping_sub(1);
                    if below > before { [0; 16] } else { below }
                }
                _ => [0; 16],
            };
            section[at..at + 16].copy_from_slice(&session);
            field(535, "check", index as u64)
        }
        2 => {
            let kind = tc.draw(gs::sampled_from(vec![0u8, 3, 4, 127, 255]));
            section[at + 24] = kind;
            field(535, "check", index as u64)
        }
        _ => {
            let outcome = tc.draw(gs::sampled_from(vec![2u8, 7, 11, 13, 15, 20, 22, 255]));
            section[at + 25] = outcome;
            field(535, "check", index as u64)
        }
    }
}

/// L1.15: a Check with one drawn rule broken is refused with that rule's refusal, its field and its index.
#[hegel::test]
fn test_a_check_breaking_one_rule_is_refused_with_that_rule(tc: TestCase) {
    let drawn = checks(&tc, 1, 16, tc.draw(gs::booleans()));
    let mut bytes = section(&drawn);
    let mut fields = command_header(&[]);
    let refusal = match within(&tc, 0, 3) {
        0 => {
            let at = within(&tc, 0, 127);
            fields[at] = nonzero_byte(&tc);
            reserved(at)
        }
        1 => {
            bytes.extend(vec![0xEE; within(&tc, 1, 31)]);
            section_length(535, 0, bytes.len(), 32)
        }
        2 => {
            bytes.clear();
            count(535, "checks", 0, 1)
        }
        _ => {
            let index = within(&tc, 0, drawn.len() - 1);
            break_check(&tc, &drawn, &mut bytes, index, within(&tc, 0, 3))
        }
    };
    let laid = laid_out(fields, bytes);
    assert!(laid.is_some());
    assert_eq!(
        outcome::<CheckView>(&laid.unwrap_or_default()),
        Err(refusal)
    );
}

/// L1.22: with two rules of the Order line broken, the refusal is that of the earlier one: the command header before
/// the section's length, the length before any check, a check before every later one, and within one check the
/// padding, the session, the kind, the outcome.
#[hegel::test]
fn test_a_check_breaking_two_rules_is_refused_with_the_earlier(tc: TestCase) {
    let drawn = checks(&tc, 2, 16, false);
    let mut bytes = section(&drawn);
    let mut fields = command_header(&[]);
    // A rule's place in the order: 0 the command header, 1 the length, then check by check, aspect by aspect.
    let places = 2 + 4 * drawn.len();
    let first = within(&tc, 0, places - 2);
    let second = within(&tc, first + 1, places - 1);
    let mut refusals = Vec::new();
    // The later rule is broken first, so the earlier one's bytes are the last written where they share a check.
    for place in [second, first] {
        let refusal = match place {
            0 => {
                let at = within(&tc, 0, 127);
                fields[at] = nonzero_byte(&tc);
                reserved(at)
            }
            1 => {
                bytes.extend(vec![0xEE; within(&tc, 1, 31)]);
                section_length(535, 0, bytes.len(), 32)
            }
            _ => {
                let (index, aspect) = ((place - 2) / 4, (place - 2) % 4);
                break_check(&tc, &drawn, &mut bytes, index, aspect)
            }
        };
        refusals.push(refusal);
    }
    let laid = laid_out(fields, bytes);
    assert!(laid.is_some());
    assert_eq!(
        outcome::<CheckView>(&laid.unwrap_or_default()).err(),
        refusals.pop()
    );
}

/// L1.23: the builder refuses exactly when the view refuses the same checks laid out by hand, with the same refusal:
/// no check, a session of all zero, a session repeated, and two sessions out of order.
#[hegel::test]
fn test_a_check_builder_refuses_what_its_view_refuses(tc: TestCase) {
    let mut drawn = checks(&tc, 2, 16, tc.draw(gs::booleans()));
    let index = within(&tc, 1, drawn.len() - 1);
    let broken = within(&tc, 0, 4);
    match broken {
        1 => drawn.clear(),
        2 => drawn[index].0 = [0; 16],
        3 => drawn[index].0 = drawn[index - 1].0,
        4 => drawn.swap(index - 1, index),
        _ => (),
    }
    let expected = laid_out(command_header(&[]), section(&drawn));
    let body = Check::new(owned(&drawn));
    assert_builds_as_laid_out::<_, CheckView>(&body, &expected);
    assert_eq!(built(&body).is_err(), broken != 0);
}

/// L1.25: the requester's check refuses a reply whose drawn check holds an `outcome` of 0 or is not the check sent
/// at its index, naming the check, and a reply of another number of checks before any check; the system group's check
/// refuses a request whose drawn check holds an outcome.
#[hegel::test]
fn test_a_reply_is_checked_against_its_request(tc: TestCase) {
    let request = checks(&tc, 2, 16, false);
    let index = within(&tc, 0, request.len() - 1);
    let answer = |fields: &Fields, outcome: u8| (fields.0, fields.1, fields.2, outcome);
    let reply: Vec<Fields> = request
        .iter()
        .map(|fields| answer(fields, tc.draw(gs::sampled_from(vec![1u8, 12, 14, 21]))))
        .collect();
    let checked = |reply: &[Fields], sent: &[Fields]| {
        let laid = laid_out(command_header(&[]), section(reply)).unwrap_or_default();
        let sent = owned(sent);
        viewed::<CheckView, _>(&laid, |view| view.check_reply(&sent).map_err(|e| e.0))
    };
    assert_eq!(checked(&reply, &request), Ok(Ok(())));

    // The reply's check holds no outcome.
    let mut unanswered = reply.clone();
    unanswered[index].3 = 0;
    let refused = Ok(Err(field(535, "check", index as u64)));
    assert_eq!(checked(&unanswered, &request), refused);

    // The check sent at the index differs from the reply's in its session, its principal or its kind.
    let mut sent = request.clone();
    match within(&tc, 0, 2) {
        0 => sent[index].0[within(&tc, 0, 15)] ^= nonzero_byte(&tc),
        1 => sent[index].1 ^= tc.draw(gs::integers::<u64>().min_value(1)),
        _ => sent[index].2 ^= 3,
    }
    assert_eq!(checked(&reply, &sent), refused);

    // Another number of checks is refused before any check, the unanswered one included.
    let shorter = &request[..request.len() - 1];
    assert_eq!(
        checked(&unanswered, shorter),
        Ok(Err(count(
            535,
            "checks",
            request.len() as u64,
            shorter.len() as u64
        )))
    );

    // The system group refuses a request holding an outcome, and takes one holding none.
    let as_request = |checks: &[Fields]| {
        let laid = laid_out(command_header(&[]), section(checks)).unwrap_or_default();
        viewed::<CheckView, _>(&laid, |view| view.check_request().map_err(|e| e.0))
    };
    assert_eq!(as_request(&request), Ok(Ok(())));
    let mut answered = request.clone();
    answered[index].3 = reply[index].3;
    assert_eq!(as_request(&answered), refused);
}
