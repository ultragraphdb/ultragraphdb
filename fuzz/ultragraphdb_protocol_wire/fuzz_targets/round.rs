/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as the body of a Round frame: the view never panics, accepts only records that read on their own and
//! rebuilds an accepted body byte for byte; the naming of the transactions under a damaged range never panics and
//! names only transactions of the round.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, Decision, FrameView, Header, RoundHeader, RoundView,
    WriteSetRecordView,
};

const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

fuzz_target!(|data: &[u8]| {
    // The first byte: the transaction count of the round header; the rest, the body.
    let Some((count, body)) = data.split_first() else {
        return;
    };
    let range_start = body.len() / 3;
    let named = RoundView::transactions_overlapping(body, range_start..body.len());
    assert!(named.windows(2).all(|w| w[0] < w[1]));
    let header = Header::new(CLUSTER, Command::Round)
        .with_command_header(RoundHeader::new(7, 9, 1, 100, u32::from(*count)).encode());
    let Ok(frame) = header.clone().encode_frame(body) else {
        return;
    };
    let view = FrameView::validate(&frame, CLUSTER).expect("a built frame validates");
    let Ok(round) = RoundView::from_frame(&view) else {
        return;
    };
    assert_eq!(round.decisions().len(), usize::from(*count));
    let committed = round
        .decisions()
        .iter()
        .filter(|d| **d == Decision::Committed)
        .count();
    assert_eq!(committed, round.records().len());
    for record in round.records() {
        assert_eq!(WriteSetRecordView::new(record.bytes()).as_ref(), Ok(record));
    }
    assert!(
        named
            .iter()
            .all(|t| (*t as usize) < round.decisions().len())
    );
    let rebuilt = round
        .to_body()
        .expect("an accepted round reads back")
        .frame(header)
        .expect("it fits");
    assert_eq!(rebuilt, frame);
});
