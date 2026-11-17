/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Ended (`PROTOCOL.md` §13.25): a command header of all zero and one section of 16-byte sessions. Order: command
//! header bytes zero; sections; section 0 a whole number of 16 bytes, at least one session; each not all zero and above
//! the one before.

use hegel::TestCase;
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{Command, Ended, EndedView};

use crate::utils::layout::{
    assert_builds_as_laid_out, built, command_header, count, field, frame, nonzero_byte, outcome,
    read_back, reserved, section_length, sections, sessions, viewed, within,
};

/// The Ended frame of `section` laid out by hand under the command header `fields`.
fn laid_out(fields: [u8; 128], section: Vec<u8>) -> Option<Vec<u8>> {
    frame(Command::Ended, fields, &sections(&[section]), 0)
}

/// L1.4: an Ended of 1 to 64 drawn sessions is the bytes laid out by hand, reads back to its sessions and builds again
/// to the same bytes.
#[hegel::test]
fn test_an_ended_round_trips(tc: TestCase) {
    let drawn = sessions(&tc, 1, 64);
    let expected = laid_out(command_header(&[]), drawn.concat());
    let body = Ended::new(drawn.clone());
    assert!(expected.is_some());
    assert_eq!(built(&body).ok(), expected);
    let bytes = expected.unwrap_or_default();
    assert_eq!(
        viewed::<EndedView, _>(&bytes, |view| view.sessions().to_vec()),
        Ok(drawn)
    );
    let again = read_back::<EndedView>(&bytes);
    assert_eq!(again, Some(body));
    assert_eq!(again.and_then(|rebuilt| built(&rebuilt).ok()), Some(bytes));
}

/// Session `index` of `section` broken: all zero, equal to the one before or below it; the refusal it gives.
fn break_session(
    tc: &TestCase,
    drawn: &[[u8; 16]],
    section: &mut [u8],
    index: usize,
) -> ProtocolErrorEnum {
    let previous = index.checked_sub(1).map(|before| drawn[before]);
    let session = match (within(tc, 0, 2), previous) {
        (1, Some(before)) => before,
        (2, Some(before)) => {
            let mut below = before;
            below[15] = below[15].wrapping_sub(1);
            if below > before { [0; 16] } else { below }
        }
        _ => [0; 16],
    };
    section[16 * index..16 * index + 16].copy_from_slice(&session);
    field(536, "session", index as u64)
}

/// L1.16: an Ended with one drawn rule broken is refused with that rule's refusal and its index.
#[hegel::test]
fn test_an_ended_breaking_one_rule_is_refused_with_that_rule(tc: TestCase) {
    let drawn = sessions(&tc, 1, 16);
    let mut bytes = drawn.concat();
    let mut fields = command_header(&[]);
    let refusal = match within(&tc, 0, 3) {
        0 => {
            let at = within(&tc, 0, 127);
            fields[at] = nonzero_byte(&tc);
            reserved(at)
        }
        1 => {
            bytes.extend(vec![0xEE; within(&tc, 1, 15)]);
            section_length(536, 0, bytes.len(), 16)
        }
        2 => {
            bytes.clear();
            count(536, "sessions", 0, 1)
        }
        _ => {
            let index = within(&tc, 0, drawn.len() - 1);
            break_session(&tc, &drawn, &mut bytes, index)
        }
    };
    let laid = laid_out(fields, bytes);
    assert!(laid.is_some());
    assert_eq!(
        outcome::<EndedView>(&laid.unwrap_or_default()),
        Err(refusal)
    );
}

/// L1.22: with two rules of the Order line broken, the refusal is that of the earlier one: the command header before
/// the section's length, the length before any session, a session before every later one.
#[hegel::test]
fn test_an_ended_breaking_two_rules_is_refused_with_the_earlier(tc: TestCase) {
    let drawn = sessions(&tc, 2, 16);
    let mut bytes = drawn.concat();
    let mut fields = command_header(&[]);
    // A rule's place in the order: 0 the command header, 1 the length, then session by session.
    let places = 2 + drawn.len();
    let first = within(&tc, 0, places - 2);
    let second = within(&tc, first + 1, places - 1);
    let mut refusals = Vec::new();
    for place in [second, first] {
        let refusal = match place {
            0 => {
                let at = within(&tc, 0, 127);
                fields[at] = nonzero_byte(&tc);
                reserved(at)
            }
            1 => {
                bytes.extend(vec![0xEE; within(&tc, 1, 15)]);
                section_length(536, 0, bytes.len(), 16)
            }
            _ => break_session(&tc, &drawn, &mut bytes, place - 2),
        };
        refusals.push(refusal);
    }
    let laid = laid_out(fields, bytes);
    assert!(laid.is_some());
    assert_eq!(
        outcome::<EndedView>(&laid.unwrap_or_default()).err(),
        refusals.pop()
    );
}

/// L1.23: the builder refuses exactly when the view refuses the same sessions laid out by hand, with the same
/// refusal: no session, a session of all zero, a session repeated, and two sessions out of order.
#[hegel::test]
fn test_an_ended_builder_refuses_what_its_view_refuses(tc: TestCase) {
    let mut drawn = sessions(&tc, 2, 16);
    let index = within(&tc, 1, drawn.len() - 1);
    let broken = within(&tc, 0, 4);
    match broken {
        1 => drawn.clear(),
        2 => drawn[index] = [0; 16],
        3 => drawn[index] = drawn[index - 1],
        4 => drawn.swap(index - 1, index),
        _ => (),
    }
    let expected = laid_out(command_header(&[]), drawn.concat());
    let body = Ended::new(drawn);
    assert_builds_as_laid_out::<_, EndedView>(&body, &expected);
    assert_eq!(built(&body).is_err(), broken != 0);
}
