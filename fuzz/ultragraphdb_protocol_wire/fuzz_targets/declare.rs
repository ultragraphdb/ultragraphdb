/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a Declare's command header and body (`PROTOCOL.md` §13.27), sealed into a frame: the view never
//! panics; a body it accepts holds declarations and no Drop, or a Drop with the target its kind names and no
//! declaration, and its versions as its kind allows. It reads back to an owned body whose frame, its names interned by
//! the builder (§4.7), builds itself again.
#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, COMMAND_HEADER_SIZE, Command, DeclareView, FrameView, Header,
    MAX_FRAME_SIZE,
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

/// The frame `bytes` read through the view and built again from its owned body.
fn rebuilt(bytes: &[u8]) -> Vec<u8> {
    let frame =
        FrameView::validate(bytes, CLUSTER, MAX_FRAME_SIZE).expect("a frame that validates");
    DeclareView::from_frame(&frame)
        .expect("a body that reads")
        .to_body()
        .expect("an accepted body reads back")
        .frame(Header::new(CLUSTER, Command::Declare).with_stream(3))
        .expect("a body its view accepts is built")
}

fuzz_target!(|data: &[u8]| {
    let Some(bytes) = sealed(Command::Declare, data) else {
        return;
    };
    let frame = FrameView::validate(&bytes, CLUSTER, MAX_FRAME_SIZE).expect("a sealed frame");
    let Ok(view) = DeclareView::from_frame(&frame) else {
        return;
    };
    match view.drop_kind() {
        Some(drop) => {
            assert!(view.is_empty(), "a Drop holds no declaration");
            assert_eq!(view.target().is_some(), drop.names_target());
            assert_eq!((view.built_on(), view.commit_version()), (0, 0));
        }
        None => {
            assert!(!view.is_empty(), "declarations hold at least one");
            assert!(view.target().is_none());
            assert_eq!(view.version() == 0, view.commit_version() == 0);
            assert_eq!(view.commit_version() & ((1 << 22) - 1), 0);
            for index in 0..view.len() {
                assert!(
                    view.declaration(index).is_some(),
                    "every record is a declaration"
                );
            }
        }
    }
    assert!(view.declaration(view.len()).is_none());
    let once = rebuilt(&bytes);
    assert_eq!(rebuilt(&once), once, "a Declare rebuilds to itself");
});
