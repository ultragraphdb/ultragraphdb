/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as the frames of one stream, back to back, each as long as its header's `size` says, pushed into a Describe
//! answer's assembler and a Catalog reply's (`PROTOCOL.md` §8.3, §13.7) until one refuses a frame: neither panics, and
//! neither holds more than `catalog_bytes_max` bytes of sections. An answer taken whole reads through its view, its
//! Order checked once, never panicking; one that reads back to an owned body, cut again into frames within the largest
//! frame the stream held, each within it and the last alone flagged `LAST`, assembles to the same body.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_wire::{
    BodyView, CATALOG_BYTES_MAX, CatalogFrames, Command, DescribeFrames, FLAG_LAST, FrameView,
    HEADER_SIZE, Header, MAX_FRAME_SIZE,
};

/// The golden frames' cluster, so seeds from `testdata/fuzz_seeds/continuation/` pass the cluster rule.
const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

/// A frame's `size`, a little-endian u32 at header offset 40 (`PROTOCOL.md` §2.1).
const SIZE_AT: usize = 40;

/// The frames `data` holds back to back, each as long as its `size`, up to the first that does not fit.
fn frames(mut data: &[u8]) -> Vec<&[u8]> {
    let mut frames = Vec::new();
    while let Some(size) = data.get(SIZE_AT..SIZE_AT + 4) {
        let size = u32::from_le_bytes(size.try_into().expect("four bytes")) as usize;
        let Some((frame, rest)) = data.split_at_checked(size.max(HEADER_SIZE)) else {
            break;
        };
        frames.push(frame);
        data = rest;
    }
    frames
}

/// `frames` cut within `max_frame`: each within it, the last alone flagged `LAST`, each validated and taken by `push`.
fn takes_back<E: std::fmt::Debug>(
    frames: &[Vec<u8>],
    max_frame: usize,
    mut push: impl FnMut(&FrameView<'_>) -> Result<(), E>,
) {
    assert!(!frames.is_empty(), "an answer takes a frame");
    for (i, frame) in frames.iter().enumerate() {
        assert!(frame.len() <= max_frame, "a frame within max_frame");
        let view =
            FrameView::validate(frame, CLUSTER, MAX_FRAME_SIZE).expect("a cut frame validates");
        assert_eq!(
            view.header().flags() & FLAG_LAST != 0,
            i + 1 == frames.len(),
            "the last frame alone is flagged LAST"
        );
        push(&view).expect("a cut frame is taken");
    }
}

fuzz_target!(|data: &[u8]| {
    let mut describe = DescribeFrames::new();
    let mut catalog = CatalogFrames::new();
    let (mut describing, mut cataloging) = (true, true);
    let mut largest = 0;
    for bytes in frames(data) {
        let Ok(frame) = FrameView::validate(bytes, CLUSTER, MAX_FRAME_SIZE) else {
            break;
        };
        largest = largest.max(bytes.len());
        // Each assembler reads no frame after one it refused.
        describing = describing && describe.push(&frame).is_ok();
        cataloging = cataloging && catalog.push(&frame).is_ok();
        if !describing && !cataloging {
            break;
        }
    }
    if describe.is_last()
        && let Ok(answer) = describe.view()
    {
        if let Ok(body) = answer.to_body() {
            let mut again = DescribeFrames::new();
            if let Ok(cut) = body.frames(Header::new(CLUSTER, Command::Describe), largest) {
                takes_back(&cut, largest, |f| again.push(f));
                assert!(again.is_last());
                assert_eq!(
                    again
                        .view()
                        .expect("an answer cut again reads")
                        .to_body()
                        .ok(),
                    Some(body),
                    "a Describe cut again assembles to itself"
                );
            }
        }
    }
    if let Some(reply) = catalog.view() {
        assert!(reply.bytes().len() <= CATALOG_BYTES_MAX);
        if catalog.is_last()
            && let Ok(body) = reply.to_body()
        {
            let mut again = CatalogFrames::new();
            if let Ok(cut) = body.frames(Header::new(CLUSTER, Command::Catalog), largest) {
                takes_back(&cut, largest, |f| again.push(f));
                assert!(again.is_last());
                assert_eq!(
                    again
                        .view()
                        .expect("a version cut again reads")
                        .to_body()
                        .ok(),
                    Some(body),
                    "a Catalog cut again assembles to itself"
                );
            }
        }
    }
});
