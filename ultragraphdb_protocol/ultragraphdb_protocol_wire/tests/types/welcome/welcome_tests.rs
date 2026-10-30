/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, DEFAULT_INITIAL_CREDIT, DEFAULT_MAX_STREAMS, DEFAULT_MAX_WRITE_SET,
    DEFAULT_TRANSACTION_MS, ErrorCode, Hello, HelloView, MAX_FRAME_SIZE, PROTOCOL_VERSION, Welcome,
};

use crate::utils::golden_bodies::{RELEASE, frame_of, golden};

/// The golden Welcome's server, speaking up to protocol `protocol`.
fn server(protocol: u16) -> Welcome {
    Welcome::new(protocol, RELEASE, "ultragraph 0.1")
        .with_max_streams(64)
        .with_mechanisms(["SCRAM-SHA-256"])
}

fn answer(min: u16, max: u16) -> Result<Welcome, ultragraphdb_protocol_wire::ErrorReply> {
    let frame = Hello::new(min, max, 9, MAX_FRAME_SIZE as u32, "c")
        .frame(golden("hello").header())
        .unwrap();
    let view = HelloView::from_frame(&frame_of(&frame)).unwrap();
    server(3).answer(&view)
}

#[test]
fn test_welcome_builds_the_golden_frames() {
    let welcome = server(1);
    assert_eq!(
        welcome.frame(golden("welcome").header()).unwrap(),
        golden("welcome").bytes()
    );
    assert_eq!(welcome.protocol(), 1);
    assert_eq!(welcome.release(), RELEASE);
    assert!(welcome.authenticates());
    assert_eq!(welcome.mechanisms(), ["SCRAM-SHA-256"]);
    assert_eq!(welcome.max_frame(), 1 << 20);
    assert_eq!(welcome.max_streams(), 64);
    assert_eq!(welcome.initial_credit(), 4);
    assert_eq!(welcome.max_write_set(), 1_040_384);
    assert_eq!(welcome.transaction_ms(), 10_000);
    assert_eq!(welcome.server(), "ultragraph 0.1");
    let open = Welcome::new(1, RELEASE, "ultragraph 0.1")
        .with_max_frame(1 << 19)
        .with_max_streams(64);
    assert!(!open.authenticates());
    assert!(open.mechanisms().is_empty());
    assert_eq!(
        open.frame(golden("welcome_no_auth").header()).unwrap(),
        golden("welcome_no_auth").bytes()
    );
}

#[test]
fn test_a_new_welcome_takes_the_protocols_default_limits() {
    let welcome = Welcome::new(1, 0, "s");
    assert_eq!(welcome.max_frame() as usize, MAX_FRAME_SIZE);
    assert_eq!(welcome.max_streams(), DEFAULT_MAX_STREAMS);
    assert_eq!(welcome.initial_credit(), DEFAULT_INITIAL_CREDIT);
    assert_eq!(welcome.max_write_set(), DEFAULT_MAX_WRITE_SET);
    assert_eq!(welcome.transaction_ms(), DEFAULT_TRANSACTION_MS);
    assert!(!welcome.authenticates());
    // PROTOCOL.md section 11.9's table.
    assert_eq!(
        (
            DEFAULT_MAX_STREAMS,
            DEFAULT_INITIAL_CREDIT,
            DEFAULT_MAX_WRITE_SET,
            DEFAULT_TRANSACTION_MS
        ),
        (256, 4, 1_040_384, 10_000)
    );
    // Each setter sets its field alone.
    let set = Welcome::new(1, 0, "s")
        .with_max_frame(600_000)
        .with_max_streams(7)
        .with_initial_credit(9)
        .with_max_write_set(11)
        .with_transaction_ms(13)
        .with_mechanisms(["A", "B"]);
    assert_eq!(
        (
            set.max_frame(),
            set.max_streams(),
            set.initial_credit(),
            set.max_write_set(),
            set.transaction_ms()
        ),
        (600_000, 7, 9, 11, 13)
    );
    assert_eq!(set.mechanisms(), ["A", "B"]);
    assert!(!set.with_mechanisms(Vec::<String>::new()).authenticates());
}

#[test]
fn test_the_answer_takes_the_highest_protocol_both_speak() {
    // The server speaks 1 to 3.
    assert_eq!(answer(1, 1).unwrap().protocol(), 1);
    assert_eq!(answer(1, 2).unwrap().protocol(), 2);
    assert_eq!(answer(2, 3).unwrap().protocol(), 3);
    assert_eq!(answer(1, 9).unwrap().protocol(), 3);
    assert_eq!(answer(3, 9).unwrap().protocol(), 3);
    // Everything else is the server's.
    assert_eq!(answer(1, 9).unwrap(), server(3));
    assert_eq!(answer(1, 2).unwrap(), server(2));
}

#[test]
fn test_a_range_above_the_server_is_refused_naming_its_highest() {
    let refused = answer(4, 9).unwrap_err();
    assert_eq!(refused.code(), ErrorCode::ProtocolVersion);
    assert_eq!(refused.detail(), 3);
    assert!(!refused.retryable());
    assert!(refused.message().contains('3'), "{}", refused.message());
    assert_eq!(answer(4, 4).unwrap_err().code(), ErrorCode::ProtocolVersion);
    assert_eq!(PROTOCOL_VERSION, 1);
}
