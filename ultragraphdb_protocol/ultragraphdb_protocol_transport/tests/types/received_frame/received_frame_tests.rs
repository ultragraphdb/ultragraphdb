/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_transport::{BufferPool, Framer};
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, Credit, ErrorCode, ErrorReply, FLAG_LAST, Header,
};

use crate::utils::feed::feed;

#[test]
fn test_a_received_frame_reads_its_header_and_validates_its_body() {
    let pool = BufferPool::new(2, 1 << 20);
    let frame = Credit::new(3)
        .frame(
            Header::new(7, Command::Credit)
                .with_stream(9)
                .with_flags(FLAG_LAST),
        )
        .unwrap();
    // A frame whose chunk is corrupted: its header is sound, its body is not.
    let mut corrupt = ErrorReply::new(ErrorCode::Busy, "busy")
        .frame(Header::new(7, Command::Error).with_stream(2))
        .unwrap();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    let stream = [frame.clone(), corrupt].concat();
    let (mut framer, mut at) = (Framer::new(7), 0);
    let received = feed(&mut framer, &pool, &stream, &mut at, 1 << 20).unwrap();
    assert_eq!(received.command(), 14);
    assert_eq!(received.stream(), 9);
    assert_eq!(received.flags(), FLAG_LAST);
    assert_eq!(received.view().unwrap().command(), Command::Credit);
    let damaged = feed(&mut framer, &pool, &stream, &mut at, 1 << 20).unwrap();
    assert_eq!(damaged.command(), 16);
    assert!(matches!(
        damaged.view().unwrap_err().0,
        ProtocolErrorEnum::Chunk { index: 0, .. }
    ));
}
