/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a Defaults' command header and body (`PROTOCOL.md` §13.28), sealed into a frame: the view never
//! panics; a request it accepts holds no predicate, and a reply's predicates ascend by their texts' bytes. An accepted
//! body reads back to an owned body that builds the same frame, byte for byte; the assembler takes the frame as its
//! view does; and the answer, cut again into frames within a size the input chooses, each within it and the last alone
//! flagged `LAST`, assembles to the same predicates.
#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, COMMAND_HEADER_SIZE, Command, DefaultsFrames, DefaultsView, FLAG_LAST,
    FrameView, Header, MAX_FRAME_SIZE,
};

/// The golden frames' cluster.
const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

/// `data` as a command header, its first 128 bytes, zero where it is shorter, and a body, the rest, sealed into a frame
/// of `command` whose checksums hold, so every input reaches the body's checks; `None` for a body past the largest
/// frame.
fn sealed(command: Command, data: &[u8]) -> Option<Vec<u8>> {
    let (fields, body) = data.split_at(data.len().min(COMMAND_HEADER_SIZE));
    let mut command_header = [0u8; COMMAND_HEADER_SIZE];
    command_header[..fields.len()].copy_from_slice(fields);
    Header::new(CLUSTER, command)
        .with_stream(3)
        .with_command_header(command_header)
        .encode_frame(body)
        .ok()
}

fuzz_target!(|data: &[u8]| {
    let Some(bytes) = sealed(Command::Defaults, data) else {
        return;
    };
    let frame = FrameView::validate(&bytes, CLUSTER, MAX_FRAME_SIZE).expect("a sealed frame");
    let read = DefaultsView::from_frame(&frame);
    let mut taken = DefaultsFrames::new();
    assert_eq!(
        taken.push(&frame).is_ok(),
        read.is_ok(),
        "the assembler takes a first frame as its view does"
    );
    let Ok(view) = read else {
        return;
    };
    let predicates: Vec<&str> = (0..view.len())
        .map(|index| view.predicate(index).expect("a predicate within the count"))
        .collect();
    assert!(
        !view.is_request() || predicates.is_empty(),
        "a request holds no predicate"
    );
    assert!(
        predicates
            .windows(2)
            .all(|pair| pair[0].as_bytes() < pair[1].as_bytes()),
        "predicates ascend"
    );
    assert_eq!(taken.predicates(), predicates.as_slice());
    let body = view.to_body().expect("an accepted body reads back");
    let header = Header::new(CLUSTER, Command::Defaults).with_stream(3);
    assert_eq!(
        body.frame(header.clone())
            .expect("a body its view accepts is built"),
        bytes,
        "a Defaults has one encoding"
    );
    // Cut again within a frame of 296 bytes, the least holding a predicate of no byte, and what the input's length adds.
    let max_frame = 296 + data.len() % 512;
    let Ok(frames) = body.frames(header, max_frame) else {
        return;
    };
    let mut again = DefaultsFrames::new();
    for (index, cut) in frames.iter().enumerate() {
        assert!(cut.len() <= max_frame || frames.len() == 1 && predicates.is_empty());
        let part =
            FrameView::validate(cut, CLUSTER, MAX_FRAME_SIZE).expect("a cut frame validates");
        assert_eq!(
            part.header().flags() & FLAG_LAST != 0,
            index + 1 == frames.len(),
            "the last frame alone is flagged LAST"
        );
        again.push(&part).expect("a cut frame is taken");
    }
    assert!(again.is_last());
    assert_eq!(again.predicates(), predicates.as_slice());
});
