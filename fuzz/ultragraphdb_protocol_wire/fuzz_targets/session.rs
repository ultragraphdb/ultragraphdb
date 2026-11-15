/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a Session's command header and body (`PROTOCOL.md` §13.23), sealed into a frame: the view never
//! panics; a body it accepts names a session, holds no section, and reads back to an owned body that builds the same
//! frame, byte for byte.
#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, COMMAND_HEADER_SIZE, Command, FrameView, Header, MAX_FRAME_SIZE,
    SessionView,
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
    let Some(bytes) = sealed(Command::Session, data) else {
        return;
    };
    let frame = FrameView::validate(&bytes, CLUSTER, MAX_FRAME_SIZE).expect("a sealed frame");
    let Ok(view) = SessionView::from_frame(&frame) else {
        return;
    };
    assert_ne!(view.session(), [0; 16], "a Session names a session");
    assert!(frame.body().is_empty(), "a Session holds no section");
    let body = view.to_body().expect("an accepted body reads back");
    let rebuilt = body
        .frame(Header::new(CLUSTER, Command::Session).with_stream(3))
        .expect("a body its view accepts is built");
    assert_eq!(rebuilt, bytes, "a Session has one encoding");
});
