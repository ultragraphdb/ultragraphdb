/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a Check's command header and body (`PROTOCOL.md` §13.24), sealed into a frame: the view never panics;
//! a body it accepts holds at least one check, its sessions ascending and none all zero, and reads back to an owned
//! body that builds the same frame, byte for byte. The receiver's and the requester's checks never panic, and a reply
//! is taken as the reply to itself exactly when every check holds an outcome.
#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, COMMAND_HEADER_SIZE, CheckOutcome, CheckView, Command, FrameView,
    Header, MAX_FRAME_SIZE, SessionCheck,
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
    let Some(bytes) = sealed(Command::Check, data) else {
        return;
    };
    let frame = FrameView::validate(&bytes, CLUSTER, MAX_FRAME_SIZE).expect("a sealed frame");
    let Ok(view) = CheckView::from_frame(&frame) else {
        return;
    };
    assert!(!view.is_empty(), "a Check holds at least one check");
    let checks: Vec<SessionCheck> = (0..view.len())
        .map(|index| view.get(index).expect("a check within the count"))
        .collect();
    assert!(view.get(view.len()).is_none());
    assert!(checks.iter().all(|check| check.session() != [0; 16]));
    assert!(
        checks
            .windows(2)
            .all(|pair| pair[0].session() < pair[1].session()),
        "sessions ascend"
    );
    let answered = |check: &SessionCheck| check.outcome() != CheckOutcome::Unanswered;
    assert_eq!(
        view.check_request().is_ok(),
        !checks.iter().any(answered),
        "a request holds no outcome"
    );
    assert_eq!(
        view.check_reply(&checks).is_ok(),
        checks.iter().all(answered),
        "a reply answers every check it was sent"
    );
    let body = view.to_body().expect("an accepted body reads back");
    assert_eq!(body.checks(), checks.as_slice());
    let rebuilt = body
        .frame(Header::new(CLUSTER, Command::Check).with_stream(3))
        .expect("a body its view accepts is built");
    assert_eq!(rebuilt, bytes, "a Check has one encoding");
});
