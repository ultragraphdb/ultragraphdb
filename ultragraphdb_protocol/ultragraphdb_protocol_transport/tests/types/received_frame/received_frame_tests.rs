/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_transport::{BufferPool, Framer};
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, Credit, ErrorCode, ErrorReply, FLAG_LAST, Header, MAX_FRAME_SIZE,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::feed::feed;

#[test]
fn test_a_received_frame_reads_its_header_and_validates_its_body() -> TestResult {
    let pool = BufferPool::new(2, 1 << 20);
    let frame = Credit::new(3).frame(
        Header::new(7, Command::Credit)
            .with_sequence(1)
            .with_stream(9)
            .with_flags(FLAG_LAST),
    )?;
    // A frame whose chunk is corrupted: its header is sound, its body is not.
    let mut corrupt = ErrorReply::new(ErrorCode::Busy, "busy").frame(
        Header::new(7, Command::Error)
            .with_sequence(2)
            .with_stream(2),
    )?;
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    let stream = [frame.clone(), corrupt].concat();
    let (mut framer, mut at) = (Framer::new(7, MAX_FRAME_SIZE), 0);
    let received = feed(&mut framer, &pool, &stream, &mut at, 1 << 20)?;
    assert_eq!(received.command(), 14);
    assert_eq!(received.stream(), 9);
    assert_eq!(received.flags(), FLAG_LAST);
    assert_eq!(received.view()?.command(), Command::Credit);
    let damaged = feed(&mut framer, &pool, &stream, &mut at, 1 << 20)?;
    assert_eq!(damaged.command(), 16);
    assert!(matches!(
        damaged.view().err().context("the refusal")?.0,
        ProtocolErrorEnum::Chunk { index: 0, .. }
    ));
    Ok(())
}

fn credit(sequence: u128, protocol: u16) -> TestResult<Vec<u8>> {
    Ok(Credit::new(1).frame(
        Header::new(7, Command::Credit)
            .with_sequence(sequence)
            .with_protocol(protocol)
            .with_stream(3),
    )?)
}

#[test]
fn test_a_frame_out_of_sequence_is_refused_after_its_chunks() -> TestResult {
    let pool = BufferPool::new(4, 1 << 20);
    let mut framer = Framer::new(7, MAX_FRAME_SIZE);
    // A frame repeated: the second copy carries 1 where 2 is due.
    let stream = [credit(1, 1)?, credit(1, 1)?, credit(3, 1)?].concat();
    let mut at = 0;
    let first = feed(&mut framer, &pool, &stream, &mut at, 1 << 20)?;
    assert_eq!(first.expected_sequence(), 1);
    assert_eq!(first.view()?.header().sequence(), 1);
    let repeated = feed(&mut framer, &pool, &stream, &mut at, 1 << 20)?;
    assert_eq!(
        repeated.view().err().context("the refusal")?.0,
        ProtocolErrorEnum::SequenceOrder {
            expected: 2,
            found: 1
        }
    );
    // A frame skipping ahead: 3 where 3 is due after the repeat is read as the second's place.
    let next = feed(&mut framer, &pool, &stream, &mut at, 1 << 20)?;
    assert_eq!(next.expected_sequence(), 3);
    assert_eq!(next.view()?.header().sequence(), 3);
    // A damaged chunk is named before the sequence.
    let mut damaged = ErrorReply::new(ErrorCode::Busy, "busy").frame(
        Header::new(7, Command::Error)
            .with_sequence(9)
            .with_stream(2),
    )?;
    let last = damaged.len() - 1;
    damaged[last] ^= 1;
    let mut fresh = Framer::new(7, MAX_FRAME_SIZE);
    let received = feed(&mut fresh, &pool, &damaged, &mut 0, 1 << 20)?;
    assert!(matches!(
        received.view().err().context("the refusal")?.0,
        ProtocolErrorEnum::Chunk { .. }
    ));
    Ok(())
}

#[test]
fn test_hello_and_welcome_take_protocol_one_and_later_frames_the_agreed_one() -> TestResult {
    let pool = BufferPool::new(4, 1 << 20);
    let mut framer = Framer::new(7, MAX_FRAME_SIZE);
    // Before the handshake agreed, every frame reads at protocol 1.
    let stream = [credit(1, 2)?].concat();
    let refused = feed(&mut framer, &pool, &stream, &mut 0, 1 << 20)?;
    assert_eq!(
        refused.view().err().context("the refusal")?.0,
        ProtocolErrorEnum::FieldValue {
            command: 14,
            field: "protocol",
            value: 2
        }
    );
    // Agreed at 2: a Credit at 2 reads, one at 1 is refused, and a Welcome stays at 1.
    let mut agreed_framer = Framer::new(7, MAX_FRAME_SIZE);
    agreed_framer.set_protocol(2);
    let welcome = ultragraphdb_protocol_wire::Welcome::new(2, 0, "s")
        .frame(Header::new(7, Command::Welcome).with_sequence(3))?;
    let agreed_stream = [credit(1, 2)?, credit(2, 1)?, welcome].concat();
    let mut at = 0;
    let agreed = feed(&mut agreed_framer, &pool, &agreed_stream, &mut at, 1 << 20)?;
    assert_eq!(agreed.view()?.header().protocol(), 2);
    assert!(matches!(
        feed(&mut agreed_framer, &pool, &agreed_stream, &mut at, 1 << 20)?
            .view()
            .err()
            .context("the refusal")?
            .0,
        ProtocolErrorEnum::FieldValue {
            field: "protocol",
            value: 1,
            ..
        }
    ));
    let welcome_received = feed(&mut agreed_framer, &pool, &agreed_stream, &mut at, 1 << 20)?;
    let view = welcome_received.view()?;
    assert_eq!(
        (view.command(), view.header().protocol()),
        (Command::Welcome, 1)
    );
    Ok(())
}

#[test]
fn test_a_frames_order_is_checked_from_its_header_without_its_chunks() -> TestResult {
    let pool = BufferPool::new(4, 1 << 20);
    let mut framer = Framer::new(7, MAX_FRAME_SIZE);
    // A Credit has no body, so the damaged frame is an Error's, its chunk broken and its header sound.
    let mut damaged = ErrorReply::new(ErrorCode::Busy, "busy").frame(
        Header::new(7, Command::Error)
            .with_sequence(1)
            .with_stream(2),
    )?;
    let last = damaged.len() - 1;
    damaged[last] ^= 1;
    let stream = [damaged, credit(1, 1)?, credit(3, 2)?].concat();
    let mut at = 0;
    let first = feed(&mut framer, &pool, &stream, &mut at, 1 << 20)?;
    assert_eq!(
        first.check_order(),
        Ok(()),
        "its chunks are left to its view"
    );
    assert!(matches!(
        first.view().err().context("the refusal")?.0,
        ProtocolErrorEnum::Chunk { .. }
    ));
    let repeated = feed(&mut framer, &pool, &stream, &mut at, 1 << 20)?;
    assert_eq!(
        repeated.check_order().err().context("the refusal")?.0,
        ProtocolErrorEnum::SequenceOrder {
            expected: 2,
            found: 1
        }
    );
    let other = feed(&mut framer, &pool, &stream, &mut at, 1 << 20)?;
    assert_eq!(
        other.check_order().err().context("the refusal")?.0,
        ProtocolErrorEnum::FieldValue {
            command: 14,
            field: "protocol",
            value: 2
        },
        "its sequence is due, its protocol is not"
    );
    Ok(())
}

#[test]
fn test_a_frame_without_flags_reads_flags_zero() -> TestResult {
    // A header built with no flags carries 0 at byte 65.
    let pool = BufferPool::new(1, 1 << 20);
    let frame = credit(1, 1)?;
    let mut at = 0;
    let received = feed(
        &mut Framer::new(7, MAX_FRAME_SIZE),
        &pool,
        &frame,
        &mut at,
        1 << 20,
    )?;
    assert_eq!(received.flags(), 0);
    Ok(())
}
