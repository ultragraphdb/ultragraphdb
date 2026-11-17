/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Subscribe's stage 5.8 field (`PROTOCOL.md` §13.3): `from` an epoch at 0, `latest` u8 at 8. Order: unused bytes
//! 9–127 zero; `from` at most 2⁴² − 1; `latest` 0 or 1, and 1 only with `from` 0; sections.

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_protocol_wire::{Command, Subscribe, SubscribeView};

use crate::utils::layout::{
    EPOCH_MAX, built, command_header, field, frame, nonzero_byte, outcome, read_back, reserved,
    sections, viewed, within,
};

/// The Subscribe frame of `from` and `latest` laid out by hand, `unused` a command-header byte set outside the fields.
fn laid_out(from: u64, latest: u8, unused: Option<(usize, u8)>) -> Option<Vec<u8>> {
    let mut header = command_header(&[(0, &from.to_le_bytes()), (8, &[latest])]);
    if let Some((at, byte)) = unused {
        header[at] = byte;
    }
    frame(Command::Subscribe, header, &sections(&[]), 0)
}

/// L1.11: a Subscribe of `latest` 0 from a drawn epoch, or of `latest` 1 from none, is the bytes laid out by hand,
/// reads back to its fields and builds again to the same bytes.
#[hegel::test]
fn test_a_subscribe_round_trips(tc: TestCase) {
    let latest = tc.draw(gs::booleans());
    let from = if latest {
        0
    } else {
        tc.draw(gs::integers::<u64>().max_value(EPOCH_MAX))
    };
    let expected = laid_out(from, u8::from(latest), None);
    let body = if latest {
        Subscribe::at_latest()
    } else {
        Subscribe::new(from)
    };
    assert_eq!((body.from(), body.latest()), (from, latest));
    assert!(expected.is_some());
    assert_eq!(built(&body).ok(), expected);
    let bytes = expected.unwrap_or_default();
    assert_eq!(
        viewed::<SubscribeView, _>(&bytes, |view| (view.from(), view.latest())),
        Ok((from, latest))
    );
    let again = read_back::<SubscribeView>(&bytes);
    assert_eq!(again, Some(body));
    assert_eq!(again.and_then(|rebuilt| built(&rebuilt).ok()), Some(bytes));
}

/// L1.21, Subscribe: one drawn rule broken is refused with that rule's refusal: an unused byte, `from` above 2⁴² − 1,
/// `latest` above 1, and `latest` 1 with a `from`; and with `from` out of range too, `from` is refused first.
#[hegel::test]
fn test_a_subscribe_breaking_a_rule_is_refused_with_that_rule(tc: TestCase) {
    let from = tc.draw(gs::integers::<u64>().min_value(1).max_value(EPOCH_MAX));
    let past = tc.draw(gs::integers::<u64>().min_value(EPOCH_MAX + 1));
    let other = tc.draw(gs::integers::<u8>().min_value(2));
    let at = within(&tc, 9, 127);
    let (laid, refusal) = match within(&tc, 0, 4) {
        0 => (
            laid_out(from, other, Some((at, nonzero_byte(&tc)))),
            reserved(at),
        ),
        1 => (laid_out(past, 0, None), field(514, "from", past)),
        2 => (laid_out(0, other, None), field(514, "latest", other.into())),
        3 => (laid_out(from, 1, None), field(514, "latest", 1)),
        _ => (laid_out(past, other, None), field(514, "from", past)),
    };
    assert!(laid.is_some());
    assert_eq!(
        outcome::<SubscribeView>(&laid.unwrap_or_default()),
        Err(refusal)
    );
}
