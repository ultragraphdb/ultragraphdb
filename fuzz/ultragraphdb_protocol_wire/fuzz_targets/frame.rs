/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any bytes as a frame: validation never panics, agrees with the header's own checks, and a frame it accepts is the
//! one its header and body build.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_protocol_wire::{FrameView, HeaderView, MAX_FRAME_SIZE};

/// The golden frames' cluster, so seeds from `testdata/fuzz_seeds/frame/` pass the cluster rule.
const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

fuzz_target!(|data: &[u8]| {
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
});
