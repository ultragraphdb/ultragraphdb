/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, Credit, ErrorCode, ErrorReply, Execute, FrameView, Header,
};

use crate::utils::golden_bodies::{CLUSTER, golden};

#[test]
fn test_frame_takes_the_command_and_command_header_from_the_body() {
    // The header names Abort; the frame is the Credit's.
    let header = Header::new(CLUSTER, Command::Abort)
        .with_stream(11)
        .with_release(0x0005_0000);
    let bytes = Credit::new(2).frame(header).unwrap();
    assert_eq!(bytes, golden("credit").bytes());
    let frame = FrameView::validate(&bytes, CLUSTER).unwrap();
    assert_eq!(frame.command(), Command::Credit);
    assert_eq!(&frame.header().command_header()[..4], &2u32.to_le_bytes());
    assert_eq!(<Credit as BodyBuilder>::COMMAND, Command::Credit);
}

#[test]
fn test_a_body_past_the_largest_frame_is_refused() {
    // 1 MiB of message: the frame exceeds 1,048,576 bytes.
    let big = ErrorReply::new(ErrorCode::Refused, "x".repeat(1 << 20));
    let refused = big.frame(Header::new(CLUSTER, Command::Error)).unwrap_err();
    assert!(matches!(
        refused.0,
        ProtocolErrorEnum::FrameTooLarge {
            limit: 1_048_576,
            ..
        }
    ));
    // 50,000 distinct IRIs: 24 bytes of record and 20 of text each, past 1 MiB.
    let mut terms = ultragraphdb_protocol_wire::TermTable::new();
    let parameters = (0..50_000)
        .map(|i| terms.iri(&format!("http://ex.org/{i:07}")))
        .collect();
    assert!(
        Execute::new(1, 0, terms, parameters, Vec::new())
            .frame(Header::new(CLUSTER, Command::Execute))
            .is_err()
    );
}
