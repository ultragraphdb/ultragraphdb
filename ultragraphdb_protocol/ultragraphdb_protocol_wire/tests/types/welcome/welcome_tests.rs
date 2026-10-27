/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, ErrorCode, Hello, HelloView, PROTOCOL_VERSION, Welcome,
};

use crate::utils::golden_bodies::{RELEASE, frame_of, golden};

fn server() -> Welcome {
    Welcome::new(3, RELEASE, 1 << 20, 64, 4, "ultragraph 0.1")
}

fn answer(min: u16, max: u16) -> Result<Welcome, ultragraphdb_protocol_wire::ErrorReply> {
    let frame = Hello::new(min, max, 9, "c")
        .frame(golden("hello").header())
        .unwrap();
    let view = HelloView::from_frame(&frame_of(&frame)).unwrap();
    server().answer(&view)
}

#[test]
fn test_welcome_builds_the_golden_frame() {
    let welcome = Welcome::new(1, RELEASE, 1 << 20, 64, 4, "ultragraph 0.1");
    assert_eq!(
        welcome.frame(golden("welcome").header()).unwrap(),
        golden("welcome").bytes()
    );
    assert_eq!(welcome.protocol(), 1);
    assert_eq!(welcome.release(), RELEASE);
    assert_eq!(welcome.max_frame(), 1 << 20);
    assert_eq!(welcome.max_streams(), 64);
    assert_eq!(welcome.initial_credit(), 4);
    assert_eq!(welcome.server(), "ultragraph 0.1");
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
    let answered = answer(1, 9).unwrap();
    assert_eq!(
        answered,
        Welcome::new(3, RELEASE, 1 << 20, 64, 4, "ultragraph 0.1")
    );
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
