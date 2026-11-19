/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! CloseRequest's stage 5.8 field (`PROTOCOL.md` §13.16): `partition` u32 at 0, 0 from the frontend, `epoch` at 8.
//! Order: unused bytes 4–7 and 16–127 zero; `partition`; `epoch`; sections.

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_protocol_wire::{CloseRequest, CloseRequestView, Command};

use crate::utils::layout::{
    EPOCH_MAX, built, command_header, field, frame, nonzero_byte, outcome, read_back, reserved,
    sections, viewed,
};

/// The CloseRequest frame laid out by hand, `unused` a command-header byte set outside the fields.
fn laid_out(partition: u32, epoch: u64, unused: Option<(usize, u8)>) -> Option<Vec<u8>> {
    let mut header = command_header(&[(0, &partition.to_le_bytes()), (8, &epoch.to_le_bytes())]);
    if let Some((at, byte)) = unused {
        header[at] = byte;
    }
    frame(Command::CloseRequest, header, &sections(&[]), 0)
}

/// L1.12: a CloseRequest of any drawn `partition`, 0 included, and an epoch of at least 1 is the bytes laid out by
/// hand, reads back to its fields and builds again to the same bytes; the frontend's constructor builds `partition`
/// 0.
#[hegel::test]
fn test_a_close_request_round_trips(tc: TestCase) {
    let partition = tc.draw(gs::sampled_from(vec![0u32, 0, 1, 7, u32::MAX]));
    let epoch = tc.draw(gs::integers::<u64>().min_value(1).max_value(EPOCH_MAX));
    let expected = laid_out(partition, epoch, None);
    let body = if partition == 0 {
        CloseRequest::compute(epoch)
    } else {
        CloseRequest::new(partition, epoch)
    };
    assert_eq!((body.partition(), body.epoch()), (partition, epoch));
    assert_eq!(CloseRequest::new(0, epoch), CloseRequest::compute(epoch));
    assert!(expected.is_some());
    assert_eq!(built(&body).ok(), expected);
    let bytes = expected.unwrap_or_default();
    assert_eq!(
        viewed::<CloseRequestView, _>(&bytes, |view| (view.partition(), view.epoch())),
        Ok((partition, epoch))
    );
    let again = read_back::<CloseRequestView>(&bytes);
    assert_eq!(again, Some(body));
    assert_eq!(again.and_then(|rebuilt| built(&rebuilt).ok()), Some(bytes));
}

/// L1.21, CloseRequest: an unused byte, and an `epoch` of 0 whatever the `partition`, is refused with its rule's
/// refusal.
#[hegel::test]
fn test_a_close_request_breaking_a_rule_is_refused_with_that_rule(tc: TestCase) {
    let partition = tc.draw(gs::sampled_from(vec![0u32, 0, 1, 7, u32::MAX]));
    let epoch = tc.draw(gs::integers::<u64>().min_value(1).max_value(EPOCH_MAX));
    let at = tc.draw(gs::sampled_from(
        (4usize..=7).chain(16..=127).collect::<Vec<usize>>(),
    ));
    let (laid, refusal) = if tc.draw(gs::booleans()) {
        (
            laid_out(partition, epoch, Some((at, nonzero_byte(&tc)))),
            reserved(at),
        )
    } else {
        (laid_out(partition, 0, None), field(527, "epoch", 0))
    };
    assert!(laid.is_some());
    assert_eq!(
        outcome::<CloseRequestView>(&laid.unwrap_or_default()),
        Err(refusal)
    );
}
