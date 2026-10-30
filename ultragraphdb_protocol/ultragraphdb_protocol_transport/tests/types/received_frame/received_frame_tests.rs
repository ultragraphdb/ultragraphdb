/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_transport::{BufferPool, Framer};
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, Credit, ErrorCode, ErrorReply, FLAG_LAST, Header, MAX_FRAME_SIZE,
};

use crate::utils::feed::feed;

#[test]
fn test_a_received_frame_reads_its_header_and_validates_its_body() {
    let pool = BufferPool::new(2, 1 << 20);
    let frame = Credit::new(3)
        .frame(
            Header::new(7, Command::Credit)
                .with_sequence(1)
                .with_stream(9)
                .with_flags(FLAG_LAST),
        )
        .unwrap();
    // A frame whose chunk is corrupted: its header is sound, its body is not.
    let mut corrupt = ErrorReply::new(ErrorCode::Busy, "busy")
        .frame(
            Header::new(7, Command::Error)
                .with_sequence(2)
                .with_stream(2),
        )
        .unwrap();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    let stream = [frame.clone(), corrupt].concat();
    let (mut framer, mut at) = (Framer::new(7, MAX_FRAME_SIZE), 0);
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

fn credit(sequence: u128, protocol: u16) -> Vec<u8> {
    Credit::new(1)
        .frame(
            Header::new(7, Command::Credit)
                .with_sequence(sequence)
                .with_protocol(protocol)
                .with_stream(3),
        )
        .unwrap()
}

#[test]
fn test_a_frame_out_of_sequence_is_refused_after_its_chunks() {
    let pool = BufferPool::new(4, 1 << 20);
    let mut framer = Framer::new(7, MAX_FRAME_SIZE);
    // A frame repeated: the second copy carries 1 where 2 is due.
    let stream = [credit(1, 1), credit(1, 1), credit(3, 1)].concat();
    let mut at = 0;
    assert!(
        feed(&mut framer, &pool, &stream, &mut at, 1 << 20)
            .unwrap()
            .view()
            .is_ok()
    );
    let repeated = feed(&mut framer, &pool, &stream, &mut at, 1 << 20).unwrap();
    assert_eq!(
        repeated.view().unwrap_err().0,
        ProtocolErrorEnum::SequenceOrder {
            expected: 2,
            found: 1
        }
    );
    // A frame skipping ahead: 3 where 3 is due after the repeat is read as the second's place.
    let next = feed(&mut framer, &pool, &stream, &mut at, 1 << 20).unwrap();
    assert!(next.view().is_ok());
    // A damaged chunk is named before the sequence.
    let mut damaged = ErrorReply::new(ErrorCode::Busy, "busy")
        .frame(
            Header::new(7, Command::Error)
                .with_sequence(9)
                .with_stream(2),
        )
        .unwrap();
    let last = damaged.len() - 1;
    damaged[last] ^= 1;
    let mut fresh = Framer::new(7, MAX_FRAME_SIZE);
    let received = feed(&mut fresh, &pool, &damaged, &mut 0, 1 << 20).unwrap();
    assert!(matches!(
        received.view().unwrap_err().0,
        ProtocolErrorEnum::Chunk { .. }
    ));
}

#[test]
fn test_hello_and_welcome_take_protocol_one_and_later_frames_the_agreed_one() {
    let pool = BufferPool::new(4, 1 << 20);
    let mut framer = Framer::new(7, MAX_FRAME_SIZE);
    // Before the handshake agreed, every frame reads at protocol 1.
    let stream = [credit(1, 2)].concat();
    let refused = feed(&mut framer, &pool, &stream, &mut 0, 1 << 20).unwrap();
    assert_eq!(
        refused.view().unwrap_err().0,
        ProtocolErrorEnum::FieldValue {
            command: 14,
            field: "protocol",
            value: 2
        }
    );
    // Agreed at 2: a Credit at 2 reads, one at 1 is refused, and a Welcome stays at 1.
    let mut framer = Framer::new(7, MAX_FRAME_SIZE);
    framer.set_protocol(2);
    let welcome = ultragraphdb_protocol_wire::Welcome::new(2, 0, "s")
        .frame(Header::new(7, Command::Welcome).with_sequence(3))
        .unwrap();
    let stream = [credit(1, 2), credit(2, 1), welcome].concat();
    let mut at = 0;
    assert!(
        feed(&mut framer, &pool, &stream, &mut at, 1 << 20)
            .unwrap()
            .view()
            .is_ok()
    );
    assert!(matches!(
        feed(&mut framer, &pool, &stream, &mut at, 1 << 20)
            .unwrap()
            .view()
            .unwrap_err()
            .0,
        ProtocolErrorEnum::FieldValue {
            field: "protocol",
            value: 1,
            ..
        }
    ));
    assert!(
        feed(&mut framer, &pool, &stream, &mut at, 1 << 20)
            .unwrap()
            .view()
            .is_ok()
    );
}
