/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a frame: validation never panics, agrees with the header's own checks, and a frame it accepts is the
//! one its header and body build.
//!
//! Random bytes almost never carry the header's checksum, so each input is checked twice: as given, which exercises
//! the checksum, and with its first eight bytes replaced by the checksum of the header after them, which reaches the
//! rules behind it.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_wire::{FrameView, HEADER_SIZE, HeaderView, MAX_FRAME_SIZE, xxh3_64};

/// The golden frames' cluster, so seeds from `testdata/fuzz_seeds/frame/` pass the cluster rule.
const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

fn check(data: &[u8]) {
    let validated = FrameView::validate(data, CLUSTER, MAX_FRAME_SIZE);
    // A refusal by the header's rules is the same refusal through the view.
    let header = HeaderView::new(data).and_then(|view| view.verify(CLUSTER, MAX_FRAME_SIZE));
    match (&validated, &header) {
        (Ok(frame), Ok(header)) => {
            assert_eq!(frame.command(), header.command());
            let rebuilt = header
                .encode_frame(frame.body())
                .expect("an accepted frame fits");
            assert_eq!(rebuilt, data, "an accepted frame has one encoding");
        }
        (Err(refusal), Err(first)) => assert_eq!(refusal, first),
        (Err(_), Ok(_)) => {}
        (Ok(_), Err(e)) => panic!("validated a frame whose header fails: {e}"),
    }
}

fuzz_target!(|data: &[u8]| {
    check(data);
    if let Some(tail) = data.get(8..HEADER_SIZE) {
        let mut sealed = data.to_vec();
        sealed[..8].copy_from_slice(&xxh3_64(tail).to_le_bytes());
        check(&sealed);
    }
});
