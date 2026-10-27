/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Any body with any `checksum_body` (the input's first 8 bytes, little-endian): the chunk checks never panic, the
//! layout they use adds up, and a body they accept is the one its data encodes.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{ChunkLayout, ChunkedBody};

fuzz_target!(|data: &[u8]| {
    let (checksum_body, body) = match data.split_first_chunk::<8>() {
        Some((head, body)) => (u64::from_le_bytes(*head), body),
        None => (0, data),
    };
    match ChunkLayout::of_body(body.len()) {
        Ok(layout) => {
            assert_eq!(layout.body_len(), body.len());
            assert_eq!(ChunkLayout::of_data(layout.data_len()), layout);
        }
        Err(e) => assert_eq!(e.0, ProtocolErrorEnum::BodyLength { len: body.len() }),
    }
    match ChunkedBody::verify(body, checksum_body) {
        Ok(checked) => {
            let mut out = Vec::new();
            assert_eq!(ChunkedBody::encode(checked.data(), &mut out), checksum_body);
            assert_eq!(out, body, "an accepted body has one encoding");
        }
        Err(e) => match e.0 {
            ProtocolErrorEnum::BodyLength { .. } | ProtocolErrorEnum::ChunkTable { .. } => {}
            ProtocolErrorEnum::Chunk { index, start, end } => {
                let layout =
                    ChunkLayout::of_body(body.len()).expect("a chunk failed, so the length split");
                assert_eq!(layout.chunk_range(index), Some(start..end));
            }
            other => panic!("a body check raised a header error: {other:?}"),
        },
    }
});
