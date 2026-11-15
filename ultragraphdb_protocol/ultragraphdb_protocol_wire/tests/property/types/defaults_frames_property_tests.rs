/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A Defaults answer across continuation frames (`PROTOCOL.md` §13.28): each frame within the requester's `max_frame`,
//! holding a whole term table and the first's command header, the last alone flagged `LAST`.

use hegel::TestCase;
use hegel::generators as gs;
use ultragraphdb_protocol_wire::{
    Command, Defaults, DefaultsFrames, DefaultsView, FLAG_LAST, FrameView,
};

use crate::utils::layout::{CLUSTER, FRAME_MAX, header, iris, viewed, within};

/// The frame a Defaults frame of one predicate of `len` bytes takes: 256 bytes of header, a two-entry section table,
/// a record of 24 bytes and the text (§2.3, §4.1, §4.4).
fn least_frame(len: usize) -> usize {
    256 + 16 + 24 + len
}

/// L1.24: a Defaults answer of 0 to 64 drawn IRIs, cut at a drawn frame size from the smallest holding its longest
/// predicate, reads back as the same predicates in order; every frame is within the size, holds a whole term table and
/// the first's command header, and the last alone is flagged `LAST`.
#[hegel::test]
fn test_a_defaults_answer_cut_at_any_frame_size_reads_back(tc: TestCase) {
    let predicates = iris(&tc, 0, 64);
    let tenant = tc.draw(gs::integers::<u64>());
    let position = tc.draw(gs::integers::<u64>().min_value(1));
    let longest = predicates.iter().map(String::len).max().unwrap_or(0);
    let least = least_frame(longest);
    let max_frame = if tc.draw(gs::booleans()) {
        within(&tc, least, least + 512)
    } else {
        within(&tc, least, FRAME_MAX)
    };
    let frames = Defaults::reply(tenant, position, predicates.clone())
        .frames(header(Command::Defaults), max_frame)
        .map_err(|refusal| refusal.0);
    assert!(frames.is_ok(), "{frames:?}");
    let frames = frames.unwrap_or_default();
    assert!(!frames.is_empty());

    let mut taken = DefaultsFrames::new();
    let mut read: Vec<String> = Vec::new();
    let mut headers = Vec::new();
    for (index, bytes) in frames.iter().enumerate() {
        assert!(bytes.len() <= max_frame, "{} past {max_frame}", bytes.len());
        let last = index + 1 == frames.len();
        let frame = FrameView::validate(bytes, CLUSTER, FRAME_MAX).map_err(|refusal| refusal.0);
        assert!(frame.is_ok(), "{frame:?}");
        if let Ok(frame) = frame {
            assert_eq!(frame.header().flags() & FLAG_LAST == FLAG_LAST, last);
            headers.push(*frame.header().command_header());
            assert_eq!(taken.push(&frame).map_err(|refusal| refusal.0), Ok(()));
        }
        // Each frame holds a whole term table, which its own view reads.
        let part = viewed::<DefaultsView, Vec<String>>(bytes, |view| {
            (0..view.len())
                .filter_map(|at| view.predicate(at))
                .map(str::to_string)
                .collect()
        });
        assert!(part.is_ok(), "{part:?}");
        // A frame but the last holds at least one predicate: the builder cuts no empty frame.
        assert!(last || part.as_ref().is_ok_and(|part| !part.is_empty()));
        read.extend(part.unwrap_or_default());
    }
    assert!(headers.windows(2).all(|pair| pair[0] == pair[1]));
    assert!(taken.is_last());
    assert!(!taken.is_past_bound());
    assert_eq!(
        (taken.tenant(), taken.position()),
        (Some(tenant), Some(position))
    );
    assert_eq!(taken.predicates(), predicates.as_slice());
    assert_eq!(read, predicates);
}
