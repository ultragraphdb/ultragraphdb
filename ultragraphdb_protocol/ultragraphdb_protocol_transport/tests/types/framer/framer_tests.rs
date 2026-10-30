/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::{ProtocolError, ProtocolErrorEnum};
use ultragraphdb_protocol_transport::{
    BufferPool, FrameStep, Framer, TransportError, TransportErrorEnum,
};
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, Credit, ErrorCode, ErrorReply, HEADER_SIZE, Header, Hello, MAX_FRAME_SIZE,
};

use crate::utils::feed::feed;

const CLUSTER: u64 = 7;

fn hello() -> Vec<u8> {
    Hello::new(
        1,
        1,
        3,
        MAX_FRAME_SIZE as u32,
        "a client named at some length",
    )
    .frame(Header::new(CLUSTER, Command::Hello))
    .unwrap()
}

fn credit() -> Vec<u8> {
    Credit::new(1)
        .frame(Header::new(CLUSTER, Command::Credit).with_stream(3))
        .unwrap()
}

#[test]
fn test_the_framer_asks_for_the_header_then_the_frames_size() {
    let pool = BufferPool::new(1, 4_096);
    let frame = hello();
    let mut framer = Framer::new(CLUSTER);
    assert_eq!(framer.want(), HEADER_SIZE);
    assert!(framer.between_frames());
    let mut buffer = pool.acquire().unwrap();
    let mut bytes = buffer.take();
    bytes.extend_from_slice(&frame[..100]);
    buffer.restore(bytes);
    assert_eq!(
        framer.advance(&buffer),
        Ok(FrameStep::Read(HEADER_SIZE)),
        "short: read on"
    );
    let mut bytes = buffer.take();
    bytes.extend_from_slice(&frame[100..HEADER_SIZE]);
    buffer.restore(bytes);
    assert_eq!(framer.advance(&buffer), Ok(FrameStep::Read(frame.len())));
    assert_eq!(framer.want(), frame.len());
    assert!(!framer.between_frames());
    let mut bytes = buffer.take();
    bytes.extend_from_slice(&frame[HEADER_SIZE..]);
    buffer.restore(bytes);
    assert_eq!(framer.advance(&buffer), Ok(FrameStep::Frame));
    let received = framer.finish(buffer);
    assert_eq!(received.bytes(), &frame[..]);
    assert_eq!(
        framer.want(),
        HEADER_SIZE,
        "the next frame starts at its header"
    );
}

#[test]
fn test_a_frame_of_a_header_alone_is_whole_at_its_header() {
    let pool = BufferPool::new(1, 4_096);
    let frame = credit();
    assert_eq!(frame.len(), HEADER_SIZE);
    let received = feed(&mut Framer::new(CLUSTER), &pool, &frame, &mut 0, 1).unwrap();
    assert_eq!(received.bytes(), &frame[..]);
}

#[test]
fn test_frames_back_to_back_are_read_in_order_whatever_the_reads() {
    let pool = BufferPool::new(4, 4_096);
    let stream = [hello(), credit(), hello()].concat();
    for step in [1, 3, 256, 4_096] {
        let (mut framer, mut at) = (Framer::new(CLUSTER), 0);
        let sizes: Vec<usize> = (0..3)
            .map(|_| {
                feed(&mut framer, &pool, &stream, &mut at, step)
                    .unwrap()
                    .bytes()
                    .len()
            })
            .collect();
        assert_eq!(
            sizes,
            vec![hello().len(), HEADER_SIZE, hello().len()],
            "step {step}"
        );
        assert_eq!(at, stream.len());
    }
}

#[test]
fn test_a_header_is_refused_before_its_body_is_read() {
    let pool = BufferPool::new(1, 4_096);
    let mut frame = hello();
    frame[60] ^= 1;
    let mut at = 0;
    let refused = feed(&mut Framer::new(CLUSTER), &pool, &frame, &mut at, 1 << 20).unwrap_err();
    assert!(matches!(refused.0, TransportErrorEnum::Protocol { .. }));
    assert_eq!(at, HEADER_SIZE, "no byte of the body was asked for");
    let other = feed(
        &mut Framer::new(CLUSTER + 1),
        &pool,
        &hello(),
        &mut 0,
        1 << 20,
    )
    .unwrap_err();
    assert!(
        matches!(
            other.0,
            TransportErrorEnum::Protocol {
                error: ProtocolError(ProtocolErrorEnum::WrongCluster { .. })
            }
        ),
        "{other:?}"
    );
}

#[test]
fn test_a_frame_larger_than_the_buffer_is_refused_naming_its_capacity() {
    let pool = BufferPool::new(1, 300);
    let frame = ErrorReply::new(ErrorCode::Busy, "x".repeat(100))
        .frame(Header::new(CLUSTER, Command::Error).with_stream(1))
        .unwrap();
    let refused = feed(&mut Framer::new(CLUSTER), &pool, &frame, &mut 0, 1 << 20).unwrap_err();
    assert_eq!(
        refused,
        TransportError::Protocol(ProtocolError::FrameTooLarge(frame.len(), 300))
    );
    let fits = BufferPool::new(1, frame.len());
    assert!(
        feed(&mut Framer::new(CLUSTER), &fits, &frame, &mut 0, 1 << 20).is_ok(),
        "a frame of the capacity fits"
    );
}

#[test]
fn test_a_close_between_frames_is_closed_and_inside_one_is_truncated() {
    let pool = BufferPool::new(1, 4_096);
    let frame = hello();
    let closed =
        |bytes: &[u8]| feed(&mut Framer::new(CLUSTER), &pool, bytes, &mut 0, 1 << 20).unwrap_err();
    assert_eq!(closed(&[]), TransportError::Closed());
    assert_eq!(
        closed(&frame[..3]),
        TransportError::Truncated(3, HEADER_SIZE)
    );
    assert_eq!(
        closed(&frame[..HEADER_SIZE]),
        TransportError::Truncated(HEADER_SIZE, frame.len())
    );
    assert_eq!(
        closed(&frame[..frame.len() - 1]),
        TransportError::Truncated(frame.len() - 1, frame.len())
    );
    let mut framer = Framer::new(CLUSTER);
    let _ = feed(&mut framer, &pool, &frame[..HEADER_SIZE], &mut 0, 1 << 20);
    assert!(framer.between_frames(), "a close resets the framer");
}

#[test]
fn test_a_reset_forgets_a_frame_in_progress() {
    let pool = BufferPool::new(1, 4_096);
    let frame = hello();
    let mut framer = Framer::new(CLUSTER);
    let mut buffer = pool.acquire().unwrap();
    let mut bytes = buffer.take();
    bytes.extend_from_slice(&frame[..HEADER_SIZE]);
    buffer.restore(bytes);
    assert_eq!(framer.advance(&buffer), Ok(FrameStep::Read(frame.len())));
    framer.reset();
    assert_eq!(framer.want(), HEADER_SIZE);
    assert_eq!(framer.cluster(), CLUSTER);
}
