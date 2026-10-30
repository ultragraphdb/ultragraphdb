/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, Credit, ErrorCode, ErrorReply, Execute, FrameView, Header, MAX_FRAME_SIZE,
};

use crate::utils::golden_bodies::{CLUSTER, golden};

#[test]
fn test_frame_takes_the_command_and_command_header_from_the_body() {
    // The header names Abort; the frame is the Credit's.
    let header = Header::new(CLUSTER, Command::Abort)
        .with_sequence(1)
        .with_stream(11)
        .with_release(0x0005_0000);
    let bytes = Credit::new(2).frame(header).unwrap();
    assert_eq!(bytes, golden("credit").bytes());
    let frame = FrameView::validate(&bytes, CLUSTER, MAX_FRAME_SIZE).unwrap();
    assert_eq!(frame.command(), Command::Credit);
    assert_eq!(&frame.header().command_header()[..4], &2u32.to_le_bytes());
    assert_eq!(<Credit as BodyBuilder>::COMMAND, Command::Credit);
}

#[test]
fn test_a_body_past_the_largest_frame_is_refused() {
    // 1 MiB of message. Error's body (PROTOCOL.md sections 4.1, 9.4): a 24-byte table, the 1,048,576-byte message,
    // two empty sections: 1,048,600 data bytes, 1 + ceil((1,048,600 - 3,840) / 4,096) = 257 chunks and a table of
    // 2,056, behind the 256-byte header: 1,050,912 bytes (section 2.3).
    let big = ErrorReply::new(ErrorCode::Refused, "x".repeat(1 << 20));
    let refused = big.frame(Header::new(CLUSTER, Command::Error)).unwrap_err();
    assert_eq!(
        refused.0,
        ProtocolErrorEnum::FrameTooLarge {
            size: 1_050_912,
            limit: 1_048_576
        }
    );
    // 50,000 distinct IRIs of 21 bytes, "http://ex.org/" and seven digits. Execute's body (section 8.2): a 32-byte
    // table, 1,200,000 bytes of records, 1,050,000 of text, 200,000 of parameters, an empty resume key: 2,450,032 data
    // bytes, 1 + ceil(2,446,192 / 4,096) = 599 chunks and a table of 4,792: 2,455,080 bytes with the header.
    let mut terms = ultragraphdb_protocol_wire::TermTable::new();
    let parameters = (0..50_000)
        .map(|i| terms.iri(&format!("http://ex.org/{i:07}")))
        .collect();
    assert_eq!(
        Execute::new(1, 5, 0, terms, parameters, Vec::new())
            .frame(Header::new(CLUSTER, Command::Execute))
            .map_err(|e| e.0),
        Err(ProtocolErrorEnum::FrameTooLarge {
            size: 2_455_080,
            limit: 1_048_576
        })
    );
}
