/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, DEFAULT_INITIAL_CREDIT, DEFAULT_MAX_STREAMS, DEFAULT_MAX_WRITE_SET,
    DEFAULT_TRANSACTION_MS, ErrorCode, Hello, HelloView, MAX_FRAME_SIZE, PROTOCOL_VERSION, Welcome,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::utils::golden_bodies::{RELEASE, frame_of, golden};

/// The golden Welcome's server, speaking up to protocol `protocol`.
fn server(protocol: u16) -> Welcome {
    Welcome::new(protocol, RELEASE, "ultragraph 0.1")
        .with_max_streams(64)
        .with_mechanisms(["SCRAM-SHA-256"])
}

/// The answer of a server speaking 1 to 3 to a Hello speaking `min` to `max`; the outer result fails the test when the
/// Hello cannot be built.
fn answer(
    min: u16,
    max: u16,
) -> TestResult<Result<Welcome, ultragraphdb_protocol_wire::ErrorReply>> {
    let frame =
        Hello::new(min, max, 9, MAX_FRAME_SIZE as u32, "c").frame(golden("hello")?.header())?;
    let view = HelloView::from_frame(&frame_of(&frame)?)?;
    Ok(server(3).answer(&view))
}

/// The Welcome a server speaking 1 to 3 answers a Hello speaking `min` to `max` with.
fn welcomed(min: u16, max: u16) -> TestResult<Welcome> {
    answer(min, max)?
        .map_err(|reply| TestError::new(format!("the server refused: {}", reply.message())))
}

#[test]
fn test_welcome_builds_the_golden_frames() -> TestResult {
    let welcome = server(1);
    assert_eq!(
        welcome.frame(golden("welcome")?.header())?,
        golden("welcome")?.bytes()?
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
        open.frame(golden("welcome_no_auth")?.header())?,
        golden("welcome_no_auth")?.bytes()?
    );
    Ok(())
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
fn test_the_answer_takes_the_highest_protocol_both_speak() -> TestResult {
    // The server speaks 1 to 3.
    assert_eq!(welcomed(1, 1)?.protocol(), 1);
    assert_eq!(welcomed(1, 2)?.protocol(), 2);
    assert_eq!(welcomed(2, 3)?.protocol(), 3);
    assert_eq!(welcomed(1, 9)?.protocol(), 3);
    assert_eq!(welcomed(3, 9)?.protocol(), 3);
    // Everything else is the server's.
    assert_eq!(welcomed(1, 9)?, server(3));
    assert_eq!(welcomed(1, 2)?, server(2));
    Ok(())
}

#[test]
fn test_a_range_above_the_server_is_refused_naming_its_highest() -> TestResult {
    let refused = answer(4, 9)?.err().context("a refusal")?;
    assert_eq!(refused.code(), ErrorCode::ProtocolVersion);
    assert_eq!(refused.detail(), 3);
    assert!(!refused.retryable());
    assert!(refused.message().contains('3'), "{}", refused.message());
    assert_eq!(
        answer(4, 4)?.err().context("a refusal")?.code(),
        ErrorCode::ProtocolVersion
    );
    assert_eq!(PROTOCOL_VERSION, 1);
    Ok(())
}
